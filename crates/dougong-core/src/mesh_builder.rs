//! 网格构建原语：盒体、多边形轮廓拉伸（用于栱的卷杀）、任意多面体（用于昂、耍头）。
//!
//! 所有图元直接拼进同一个 [`Mesh`]，使用平面法线（flat shading），
//! 同一构件的不同面可带不同材质编号（如斗的耳/平/欹）。

use crate::geom::{cross, normalize, Mesh, Vec3};

/// 拉伸所在平面的轴向。
#[derive(Clone, Copy, Debug)]
pub enum ExtrudeAxis {
    /// 轮廓在 (Y, Z) 平面，沿 X 拉伸（横栱）。
    X,
    /// 轮廓在 (X, Z) 平面，沿 Y 拉伸（华栱、昂身、耍头）。
    Y,
}

pub struct MeshBuilder {
    pub mesh: Mesh,
}

impl MeshBuilder {
    pub fn new() -> Self {
        Self { mesh: Mesh::new() }
    }

    /// 轴对齐盒体。`mats` 按 [+X, -X, +Y, -Y, +Z, -Z] 给六个面材质。
    pub fn add_box(&mut self, center: Vec3, half: Vec3, mats: [u16; 6]) {
        let (cx, cy, cz) = (center[0], center[1], center[2]);
        let (hx, hy, hz) = (half[0], half[1], half[2]);
        let p = [
            [cx - hx, cy - hy, cz - hz], // 0
            [cx + hx, cy - hy, cz - hz], // 1
            [cx + hx, cy + hy, cz - hz], // 2
            [cx - hx, cy + hy, cz - hz], // 3
            [cx - hx, cy - hy, cz + hz], // 4
            [cx + hx, cy - hy, cz + hz], // 5
            [cx + hx, cy + hy, cz + hz], // 6
            [cx - hx, cy + hy, cz + hz], // 7
        ];
        // (面顶点, 法线)，顶点绕法线逆时针。
        let faces: [([usize; 4], Vec3); 6] = [
            ([1, 2, 6, 5], [1.0, 0.0, 0.0]),
            ([3, 0, 4, 7], [-1.0, 0.0, 0.0]),
            ([2, 3, 7, 6], [0.0, 1.0, 0.0]),
            ([0, 1, 5, 4], [0.0, -1.0, 0.0]),
            ([4, 5, 6, 7], [0.0, 0.0, 1.0]),
            ([3, 2, 1, 0], [0.0, 0.0, -1.0]),
        ];
        for (i, (idx, n)) in faces.iter().enumerate() {
            let base = self.mesh.vertex_count() as u32;
            for &vi in idx {
                self.mesh.push_vertex(p[vi], *n, mats[i]);
            }
            self.mesh.push_tri(base, base + 1, base + 2);
            self.mesh.push_tri(base, base + 2, base + 3);
        }
    }

    /// 沿指定轴拉伸一个闭合 2D 轮廓（轮廓坐标 (横, 高)，单位分）。
    /// `length` 为拉伸全长，轮廓中心落在轴原点。
    /// 侧面用 `mat_side`，两端端面用 `mat_cap`。
    pub fn add_extrusion(
        &mut self,
        profile: &[(f32, f32)],
        axis: ExtrudeAxis,
        length: f32,
        mat_side: u16,
        mat_cap: u16,
    ) {
        let n = profile.len();
        debug_assert!(n >= 3, "轮廓至少 3 个点");
        let half_l = length * 0.5;

        let map = |u: f32, v: f32, w: f32| -> Vec3 {
            match axis {
                ExtrudeAxis::X => [w, u, v],
                ExtrudeAxis::Y => [u, w, v],
            }
        };

        // 侧面：每条轮廓边 × 拉伸方向。
        for i in 0..n {
            let j = (i + 1) % n;
            let (u0, v0) = profile[i];
            let (u1, v1) = profile[j];
            // 轮廓按逆时针给定时，外法线 = (dv, -du)。
            let du = u1 - u0;
            let dv = v1 - v0;
            let len = (du * du + dv * dv).sqrt().max(1e-9);
            let n2 = [dv / len, -du / len];
            let normal = match axis {
                ExtrudeAxis::X => [0.0, n2[0], n2[1]],
                ExtrudeAxis::Y => [n2[0], 0.0, n2[1]],
            };
            let base = self.mesh.vertex_count() as u32;
            let corners = [
                map(u0, v0, -half_l),
                map(u1, v1, -half_l),
                map(u1, v1, half_l),
                map(u0, v0, half_l),
            ];
            for c in corners {
                self.mesh.push_vertex(c, normal, mat_side);
            }
            self.mesh.push_tri(base, base + 1, base + 2);
            self.mesh.push_tri(base, base + 2, base + 3);
        }

        // 端面：耳切法三角化（轮廓允许带卷杀的凹多边形）。
        let tris = earclip(profile);
        for &w_sign in &[-half_l, half_l] {
            let normal: Vec3 = match (axis, w_sign.is_sign_negative()) {
                (ExtrudeAxis::X, true) => [-1.0, 0.0, 0.0],
                (ExtrudeAxis::X, false) => [1.0, 0.0, 0.0],
                (ExtrudeAxis::Y, true) => [0.0, -1.0, 0.0],
                (ExtrudeAxis::Y, false) => [0.0, 1.0, 0.0],
            };
            let base = self.mesh.vertex_count() as u32;
            for &(u, v) in profile {
                self.mesh.push_vertex(map(u, v, w_sign), normal, mat_cap);
            }
            for &(a, b, c) in &tris {
                if w_sign < 0.0 {
                    self.mesh.push_tri(base + a, base + c, base + b);
                } else {
                    self.mesh.push_tri(base + a, base + b, base + c);
                }
            }
        }
    }

    /// 任意凸/近凸多面体：点列 + 每面的顶点索引（面内绕序任意，法线自动按 Newell 法求）。
    pub fn add_polyhedron(&mut self, points: &[Vec3], faces: &[Vec<usize>], mat: u16) {
        for face in faces {
            let n = face_normal(points, face);
            let base = self.mesh.vertex_count() as u32;
            for &vi in face {
                self.mesh.push_vertex(points[vi], n, mat);
            }
            for k in 1..(face.len() - 1) as u32 {
                self.mesh.push_tri(base, base + k, base + k + 1);
            }
        }
    }
}

impl Default for MeshBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn face_normal(points: &[Vec3], face: &[usize]) -> Vec3 {
    let mut acc = [0.0f32; 3];
    let m = face.len();
    for i in 0..m {
        let a = points[face[i]];
        let b = points[face[(i + 1) % m]];
        acc[0] += (a[1] - b[1]) * (a[2] + b[2]);
        acc[1] += (a[2] - b[2]) * (a[0] + b[0]);
        acc[2] += (a[0] - b[0]) * (a[1] + b[1]);
    }
    normalize(acc)
}

/// 耳切法三角化。`poly` 为简单多边形（顺/逆时针均可），返回 (i, j, k) 三角形索引。
fn earclip(poly: &[(f32, f32)]) -> Vec<(u32, u32, u32)> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }
    let area_signed: f32 = (0..n)
        .map(|i| {
            let a = poly[i];
            let b = poly[(i + 1) % n];
            a.0 * b.1 - b.0 * a.1
        })
        .sum();
    let ccw = area_signed > 0.0;

    let mut indices: Vec<usize> = (0..n).collect();
    let mut tris = Vec::with_capacity(n - 2);

    let mut guard = 0;
    while indices.len() > 3 && guard < n * n {
        guard += 1;
        let m = indices.len();
        let mut clipped = false;
        for k in 0..m {
            let i0 = indices[(k + m - 1) % m];
            let i1 = indices[k];
            let i2 = indices[(k + 1) % m];
            let (a, b, c) = (poly[i0], poly[i1], poly[i2]);
            let convex = if ccw {
                cross2(a, b, c) > 0.0
            } else {
                cross2(a, b, c) < 0.0
            };
            if !convex {
                continue;
            }
            let contains_point = indices.iter().filter(|&&idx| idx != i0 && idx != i1 && idx != i2)
                .any(|&idx| point_in_tri(poly[idx], a, b, c));
            if contains_point {
                continue;
            }
            tris.push((i0 as u32, i1 as u32, i2 as u32));
            indices.remove(k);
            clipped = true;
            break;
        }
        if !clipped {
            // 数值退化兜底：强制切掉第一个顶点。
            let (i0, i1, i2) = (indices[0], indices[1], indices[2]);
            tris.push((i0 as u32, i1 as u32, i2 as u32));
            indices.remove(1);
        }
    }
    if indices.len() == 3 {
        tris.push((indices[0] as u32, indices[1] as u32, indices[2] as u32));
    }
    tris
}

fn cross2(a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> f32 {
    (b.0 - a.0) * (c.1 - a.1) - (b.1 - a.1) * (c.0 - a.0)
}

fn point_in_tri(p: (f32, f32), a: (f32, f32), b: (f32, f32), c: (f32, f32)) -> bool {
    let d1 = cross2(a, p, b);
    let d2 = cross2(b, p, c);
    let d3 = cross2(c, p, a);
    let has_neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
    let has_pos = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
    !(has_neg && has_pos)
}

/// 两个多边形轮廓的实用小工具：把矩形轮廓加进缓冲。
pub fn rect_profile(half_width: f32, bottom: f32, top: f32) -> Vec<(f32, f32)> {
    vec![
        (-half_width, bottom),
        (half_width, bottom),
        (half_width, top),
        (-half_width, top),
    ]
}

#[allow(dead_code)]
pub fn _ensure_cross_used(a: Vec3, b: Vec3) -> Vec3 {
    cross(a, b)
}
