//! 各构件的局部网格生成，坐标单位均为「分」，原点在构件底面中心。
//!
//! 材质编号约定（全模型统一，前端按此绑定木纹/彩画纹理）：
//! 0 = 通用木面，1 = 斗之耳，2 = 斗之平，3 = 斗之欹，4 = 彩画重点面。

use crate::caifen::Rules;
use crate::geom::Mesh;
use crate::mesh_builder::{ExtrudeAxis, MeshBuilder};

pub const MAT_WOOD: u16 = 0;
pub const MAT_DOU_EAR: u16 = 1;
pub const MAT_DOU_PING: u16 = 2;
pub const MAT_DOU_QI: u16 = 3;
pub const MAT_PAINT: u16 = 4;

/// 斗（栌斗/小斗通用）：耳、平、欹三段叠合，欹段向下收分。
/// `length` 沿 X，`width` 沿 Y，`height` 沿 Z。
pub fn build_dou(length: f32, width: f32, height: f32, rules: &Rules) -> Mesh {
    let mut b = MeshBuilder::new();
    let h_ear = height * rules.dou_ear_ratio;
    let h_ping = height * rules.dou_ping_ratio;
    let h_qi = height * rules.dou_qi_ratio;

    // 欹：上大下小的收分台体，用 add_polyhedron 做截头方锥。
    let top_l = length * 0.5;
    let top_w = width * 0.5;
    let bot_l = length * 0.5 * rules.dou_taper;
    let bot_w = width * 0.5 * rules.dou_taper;
    let pts = [
        [-bot_l, -bot_w, 0.0],
        [bot_l, -bot_w, 0.0],
        [bot_l, bot_w, 0.0],
        [-bot_l, bot_w, 0.0],
        [-top_l, -top_w, h_qi],
        [top_l, -top_w, h_qi],
        [top_l, top_w, h_qi],
        [-top_l, top_w, h_qi],
    ];
    let faces = vec![
        vec![0, 3, 2, 1], // 底
        vec![4, 5, 6, 7], // 顶
        vec![0, 1, 5, 4],
        vec![1, 2, 6, 5],
        vec![2, 3, 7, 6],
        vec![3, 0, 4, 7],
    ];
    b.add_polyhedron(&pts, &faces, MAT_DOU_QI);

    // 平：直段。
    b.add_box(
        [0.0, 0.0, h_qi + h_ping * 0.5],
        [top_l, top_w, h_ping * 0.5],
        [MAT_DOU_PING; 6],
    );
    // 耳：上口四周的耳沿（以整块表达，开口槽位由装配关系体现，见 DESIGN.md）。
    b.add_box(
        [0.0, 0.0, h_qi + h_ping + h_ear * 0.5],
        [top_l, top_w, h_ear * 0.5],
        [MAT_DOU_EAR; 6],
    );
    b.mesh
}

/// 栱端卷杀轮廓（(横向坐标, 高) 的闭合多边形，底面在 z=0）。
/// 中段通长保持全高，两端按瓣数逐瓣收分、降高。
fn gong_profile(total_length: f32, height: f32, rules: &Rules) -> Vec<(f32, f32)> {
    let half = total_length * 0.5;
    let bans = rules.juansha_bans.max(1) as f32;
    // 卷杀段长度约为总长的 1/4（法式瓜栱两头卷杀、中间留直段的比例化处理）。
    let sha_len = (total_length * 0.25).min(half - rules.gong_thickness * 0.5);
    let ban_len = sha_len / bans;
    // 端部总降高约为材高的 1/3，分瓣递减。
    let total_drop = height * 0.34;

    // 从左端底角起逆时针。
    let mut p: Vec<(f32, f32)> = Vec::new();
    p.push((-half, 0.0));
    // 左端卷杀：自端向中，顶线逐瓣抬高。
    for k in 0..bans as i32 {
        let u = -half + ban_len * k as f32;
        let top = height - total_drop * (1.0 - k as f32 / bans);
        p.push((u, top));
    }
    p.push((-half + sha_len, height));
    p.push((half - sha_len, height));
    // 右端卷杀。
    for k in 0..bans as i32 {
        let u = half - sha_len + ban_len * k as f32;
        let top = height - total_drop * (k as f32 + 1.0) / bans;
        p.push((u, top));
    }
    p.push((half, 0.0));
    p
}

/// 横栱（沿 X）。`gong_len` 取令/瓜/万栱长度，截面单材 10×15。
pub fn build_henggong(gong_len: f32, rules: &Rules) -> Mesh {
    let mut b = MeshBuilder::new();
    let profile = gong_profile(gong_len, rules.dan_height, rules);
    b.add_extrusion(&profile, ExtrudeAxis::X, rules.gong_thickness, MAT_WOOD, MAT_PAINT);
    b.mesh
}

/// 华栱（沿 Y 出跳），足材 10×21，端部同样卷杀。
pub fn build_huagong(length: f32, rules: &Rules) -> Mesh {
    let mut b = MeshBuilder::new();
    let profile = gong_profile(length, rules.zu_height, rules);
    b.add_extrusion(&profile, ExtrudeAxis::Y, rules.gong_thickness, MAT_WOOD, MAT_PAINT);
    b.mesh
}

/// 昂身（下昂）：绕 X 轴斜置的足材杆，外端削成昂嘴，内端留榫尾。
/// 局部坐标：原点为斗心支点，+Y 向外（斜下），-Y 为昂尾（斜上）。
pub fn build_xiaang(run_out: f32, rules: &Rules) -> Mesh {
    let angle = rules.ang_angle();
    let (s, c) = (angle.sin(), angle.cos());
    let h = rules.zu_height;
    let t = rules.gong_thickness;
    let half_t = t * 0.5;
    let tail = rules.ang_tail_length;

    // 旋转前（水平杆）的截面角点，杆轴在截面竖向的中点偏下（取 h*0.35 处为转轴，
    // 使昂底贴近下一跳斗口）。
    let pivot_z = h * 0.35;
    let rot = |y: f32, z: f32| -> (f32, f32) {
        let yy = c * y + s * (z - pivot_z);
        let zz = -s * y + c * (z - pivot_z) + pivot_z;
        (yy, zz)
    };
    let pt = |x: f32, y: f32, z: f32| -> [f32; 3] {
        let (yy, zz) = rot(y, z);
        [x, yy, zz]
    };

    // 外端昂嘴：底面在末端向上斜收，形成尖喙。
    let beak = h * 0.45;
    let raw: Vec<[f32; 3]> = vec![
        // 0-3 外端面（削尖后只剩上部）
        pt(-half_t, run_out, beak),
        pt(half_t, run_out, beak),
        pt(half_t, run_out, h),
        pt(-half_t, run_out, h),
        // 4-7 内端（榫尾，全截面）
        pt(-half_t, -tail, 0.0),
        pt(half_t, -tail, 0.0),
        pt(half_t, -tail, h),
        pt(-half_t, -tail, h),
        // 8-9 昂嘴下折点（距外端 12 分处底面上抬到 beak 高度）
        pt(-half_t, run_out - 12.0, beak),
        pt(half_t, run_out - 12.0, beak),
    ];
    let faces = vec![
        vec![0, 1, 2, 3],       // 外端面
        vec![4, 7, 6, 5],       // 内端面
        vec![0, 3, 7, 4, 8],    // -X 面
        vec![1, 5, 6, 2, 9],    // +X 面
        vec![3, 2, 6, 7],       // 顶面
        vec![0, 8, 9, 1],       // 昂嘴斜面（底）
        vec![8, 4, 5, 9],       // 底面
    ];
    let mut b = MeshBuilder::new();
    b.add_polyhedron(&raw, &faces, MAT_WOOD);
    b.mesh
}

/// 上昂：与下昂镜像，斜面朝向内跳（-Y 斜上）。
pub fn build_shangang(run_out: f32, rules: &Rules) -> Mesh {
    let mut mesh = build_xiaang(run_out, rules);
    // 镜像 Y 并反转绕序：直接把每个顶点 y 取负、翻转三角形绕序。
    for v in mesh.positions.chunks_exact_mut(3) {
        v[1] = -v[1];
    }
    for tri in mesh.indices.chunks_exact_mut(3) {
        tri.swap(1, 2);
    }
    // 法线 y 分量取负。
    for n in mesh.normals.chunks_exact_mut(3) {
        n[1] = -n[1];
    }
    mesh
}

/// 耍头（蚂蚱头）：足材水平杆，外端顶部削斜成蚂蚱头形。
/// 局部原点在底面中心，杆沿 Y，外端在 +Y。
pub fn build_shuatou(total_len: f32, rules: &Rules) -> Mesh {
    let h = rules.zu_height;
    let t = rules.gong_thickness;
    let half_t = t * 0.5;
    let half_l = total_len * 0.5;
    let tip = h - rules.mazha_tip_height; // 端部削去的高度
    let cut_len = 12.0; // 削斜段长度（分，判断值，见 DESIGN.md）

    let pts = [
        [-half_t, -half_l, 0.0],        // 0
        [half_t, -half_l, 0.0],         // 1
        [half_t, half_l, 0.0],          // 2
        [-half_t, half_l, 0.0],         // 3
        [-half_t, -half_l, h],          // 4
        [half_t, -half_l, h],           // 5
        [half_t, half_l - cut_len, h],  // 6
        [-half_t, half_l - cut_len, h], // 7
        [half_t, half_l, h - tip],      // 8
        [-half_t, half_l, h - tip],     // 9
    ];
    let faces = vec![
        vec![0, 3, 2, 1],    // 底
        vec![0, 1, 5, 4],    // 内端
        vec![2, 3, 9, 8],    // 外端（蚂蚱头正面）
        vec![0, 4, 7, 3],    // -X
        vec![1, 2, 8, 6, 5], // +X
        vec![4, 5, 6, 7],    // 顶
        vec![7, 6, 8, 9],    // 削斜面
    ];
    let mut b = MeshBuilder::new();
    b.add_polyhedron(&pts, &faces, MAT_WOOD);
    b.mesh
}
