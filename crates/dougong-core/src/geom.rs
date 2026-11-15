//! 基础三维类型与网格数据结构。
//!
//! 约定坐标系：X = 面阔方向（左右），Y = 进深方向（外跳为正），Z = 竖直向上。
//! 整个铺作的原点取栌斗底面中心，Z=0 即栌斗底。

use serde::{Deserialize, Serialize};

pub type Vec3 = [f32; 3];

/// 轴对齐包围盒，供前端计算相机取景与 SSAO 范围。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Aabb {
    pub min: Vec3,
    pub max: Vec3,
}

impl Default for Aabb {
    fn default() -> Self {
        Self {
            min: [f32::INFINITY; 3],
            max: [f32::NEG_INFINITY; 3],
        }
    }
}

/// 一个构件的局部网格。前端按 `transform` 把顶点烘焙到世界坐标，
/// glTF 导出时则作为 node 的 TRS 保留。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Mesh {
    pub positions: Vec<f32>,
    pub normals: Vec<f32>,
    /// 每顶点一个材质编号，与 [`Assembly::materials`] 对应。
    pub material_ids: Vec<u16>,
    pub indices: Vec<u32>,
}

impl Mesh {
    pub fn new() -> Self {
        Self {
            positions: Vec::new(),
            normals: Vec::new(),
            material_ids: Vec::new(),
            indices: Vec::new(),
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len() / 3
    }

    pub fn index_count(&self) -> usize {
        self.indices.len()
    }

    /// 追加一个顶点，返回其索引。
    pub fn push_vertex(&mut self, p: Vec3, n: Vec3, material: u16) -> u32 {
        let idx = self.vertex_count() as u32;
        self.positions.extend_from_slice(&p);
        self.normals.extend_from_slice(&n);
        self.material_ids.push(material);
        idx
    }

    /// 追加一个三角形（顶点顺序按右手法则朝外）。
    pub fn push_tri(&mut self, a: u32, b: u32, c: u32) {
        self.indices.extend_from_slice(&[a, b, c]);
    }
}

impl Default for Mesh {
    fn default() -> Self {
        Self::new()
    }
}

/// 平移 + 绕 Z 轴旋转（斗拱构件只需这两个自由度；昂的斜面在局部网格内做）。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Transform {
    pub translation: Vec3,
    /// 绕 Z 轴转角（弧度），0 表示华栱沿 Y 方向、横栱沿 X 方向。
    pub rotation_z: f32,
}

impl Default for Transform {
    fn default() -> Self {
        Self {
            translation: [0.0; 3],
            rotation_z: 0.0,
        }
    }
}

pub fn dot(a: Vec3, b: Vec3) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

pub fn cross(a: Vec3, b: Vec3) -> Vec3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn normalize(a: Vec3) -> Vec3 {
    let len = (dot(a, a)).sqrt().max(1e-9);
    [a[0] / len, a[1] / len, a[2] / len]
}

/// 把局部点按 transform 变换到世界坐标。
pub fn apply_transform(t: &Transform, p: Vec3) -> Vec3 {
    let (s, c) = (t.rotation_z.sin(), t.rotation_z.cos());
    [
        c * p[0] - s * p[1] + t.translation[0],
        s * p[0] + c * p[1] + t.translation[1],
        p[2] + t.translation[2],
    ]
}

pub fn rotate_z(n: Vec3, angle: f32) -> Vec3 {
    let (s, c) = (angle.sin(), angle.cos());
    [c * n[0] - s * n[1], s * n[0] + c * n[1], n[2]]
}
