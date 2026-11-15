//! 装配逻辑：按铺作类型把构件分层定位，表达榫卯咬合关系。
//!
//! # 分层模型（自下而上）
//! - 第 0 层：栌斗。
//! - 第 1..=n 层：每跳一层，华栱/昂足材（21 分）骑在下层斗口上，
//!   跳头置交互斗承托上层；与跳头同层施横栱（瓜栱→万栱，最外为令栱），
//!   横栱两端与中心置散斗/齐心斗。
//! - 最上层：耍头与令栱，撩檐方向的终点。
//!
//! 几何上不做布尔开槽（课堂展示模型，避免重网格），而是让栱/昂底嵌入斗耳上口
//! 2 分，模拟「榫头入斗口」的咬合；该取舍写进 DESIGN.md。

use crate::caifen::{CaiFen, CaiGrade, Rules};
use crate::components;
use crate::geom::{apply_transform, Aabb, Mesh, Transform, Vec3};
use crate::model::{AngStyle, Assembly, AssemblyParams, BracketSetType, Component, ComponentKind, MaterialStyle, MaterialSlot};

/// 生成整朵铺作（坐标单位：米）。
pub fn build_assembly(params: &AssemblyParams) -> Assembly {
    let grade = CaiGrade::from_number(params.cai_grade).unwrap_or(CaiGrade::Wu);
    let cf = CaiFen::new(grade);
    let rules = Rules::yingzao_fashi();
    let jumps = params.bracket_set.outer_jumps() as i32;

    let mut asm = Assembly {
        params: params.clone(),
        fen_m: cf.fen_m,
        ..Default::default()
    };
    asm.materials = materials(params.material_style);

    let mut comps: Vec<Component> = Vec::new();
    let mut next_id = 0u32;
    let mut push = |comps: &mut Vec<Component>,
                    name: String,
                    kind: ComponentKind,
                    layer: u8,
                    explode_dir: Vec3,
                    t: Transform,
                    mesh_fen: Mesh| {
        // 网格顶点与平移量统一从「分」换算到米；旋转角无量纲，保持不变。
        let mesh = scale_mesh(&mesh_fen, cf.fen_m);
        let transform = Transform {
            translation: [
                t.translation[0] * cf.fen_m,
                t.translation[1] * cf.fen_m,
                t.translation[2] * cf.fen_m,
            ],
            rotation_z: t.rotation_z,
        };
        comps.push(Component {
            id: next_id,
            name,
            kind,
            layer,
            explode_dir,
            transform,
            mesh,
        });
        next_id += 1;
    };

    // —— 第 0 层：栌斗 ——
    let ludou_h = rules.ludou_height;
    let ludou_mesh = components::build_dou(
        rules.ludou_length,
        rules.ludou_width,
        ludou_h,
        &rules,
    );
    push(
        &mut comps,
        "栌斗".to_string(),
        ComponentKind::LuDou,
        0,
        [0.0, 0.0, -1.0],
        Transform::default(),
        ludou_mesh,
    );

    if params.bracket_set == BracketSetType::DoukouTiao {
        // 斗口跳：栌斗上直接施令栱与交互斗，无出跳华栱。
        let z = ludou_h - 2.0;
        add_henggong_and_dou(
            &mut comps, &mut push, &rules, z, 0.0,
            rules.linggong_length, "令栱", 1,
        );
        asm.components = comps;
        asm.bounds = compute_bounds(&asm.components);
        asm.notes = build_notes(params, &cf, &rules);
        return asm;
    }

    // —— 出跳层 ——
    // 方向集合：外跳恒有；里跳按开关生成。
    let mut dirs: Vec<(i32, &'static str)> = vec![(1, "外")];
    if params.include_inner_jumps {
        dirs.push((-1, "里"));
    }

    for jump in 1..=jumps {
        let layer = jump as u8;
        // 足材层底：栌斗顶起，每层抬足材 21 分；嵌入斗口 2 分。
        let z = ludou_h + (jump - 1) as f32 * rules.zu_height - 2.0;
        // 本跳跳头距中心的水平距离。
        let head_y = jump as f32 * rules.jump;
        let outer_last = jump == jumps;

        for &(dir_sign, dir_name) in &dirs {
            // 该方向最后一跳是否用昂（仅外跳、且选择了昂制）。
            let use_ang = outer_last
                && dir_sign == 1
                && params.ang_style != AngStyle::None;

            if use_ang {
                let mesh_fen = match params.ang_style {
                    AngStyle::ShangAng => components::build_shangang(rules.jump, &rules),
                    _ => components::build_xiaang(rules.jump, &rules),
                };
                let name = match params.ang_style {
                    AngStyle::ShangAng => format!("上昂（{}跳）", dir_name),
                    _ => format!("下昂（{}跳）", dir_name),
                };
                // 昂的支点在本跳交互斗位置，局部原点即支点。
                let t = Transform {
                    translation: [0.0, dir_sign as f32 * head_y, z],
                    rotation_z: 0.0,
                };
                push(
                    &mut comps, name, ComponentKind::Ang, layer,
                    [0.0, dir_sign as f32, 0.35], t, mesh_fen,
                );
            } else {
                // 华栱：自中心伸到本跳跳头，长度取 head_y + 半材厚（出头）。
                let gong_len = head_y + rules.gong_thickness * 0.5;
                let mesh_fen = components::build_huagong(gong_len, &rules);
                // 局部轮廓以 y=0 为中心，需要把构件中心移到 head_y/2 处。
                let t = Transform {
                    translation: [0.0, dir_sign as f32 * head_y * 0.5, z],
                    rotation_z: 0.0,
                };
                push(
                    &mut comps,
                    format!("华栱（{}跳{}）", jump, dir_name),
                    ComponentKind::HuaGong,
                    layer,
                    [0.0, dir_sign as f32, 0.35],
                    t,
                    mesh_fen,
                );
            }

            // 跳头交互斗（昂或华栱之上）。
            let dou_z = z + rules.zu_height - 2.0;
            let xd = components::build_dou(
                rules.xiaodou_length,
                rules.xiaodou_width,
                rules.xiaodou_height,
                &rules,
            );
            push(
                &mut comps,
                format!("交互斗（{}跳{}）", jump, dir_name),
                ComponentKind::XiaoDou,
                layer,
                [0.0, dir_sign as f32, 0.6],
                Transform {
                    translation: [0.0, dir_sign as f32 * head_y, dou_z],
                    ..Default::default()
                },
                xd,
            );
        }

        // 同层横栱：最外跳用令栱；其余按瓜栱→万栱轮换（法式「瓜子栱上施慢栱」
        // 的逐层简化：奇跳瓜栱、偶跳万栱）。
        let (heng_len, heng_name) = if jump == jumps {
            (rules.linggong_length, "令栱")
        } else if jump % 2 == 1 {
            (rules.guagong_length, "瓜栱")
        } else {
            (rules.wanggong_length, "万栱")
        };
        let heng_z = z;
        add_henggong_and_dou(
            &mut comps, &mut push, &rules, heng_z, 0.0, heng_len, heng_name, layer,
        );
    }

    // —— 最上层：耍头（与令栱同层，向外伸出）——
    let top_layer = jumps as u8 + 1;
    let top_z = ludou_h + (jumps - 1) as f32 * rules.zu_height - 2.0;
    let shua_len = rules.jump + rules.shuatou_overhang;
    let shua_mesh = components::build_shuatou(shua_len, &rules);
    push(
        &mut comps,
        "耍头（蚂蚱头）".to_string(),
        ComponentKind::ShuaTou,
        top_layer,
        [0.0, 1.0, 0.5],
        Transform {
            translation: [0.0, rules.jump * 0.5 + rules.shuatou_overhang * 0.25, top_z],
            ..Default::default()
        },
        shua_mesh,
    );

    asm.components = comps;
    asm.bounds = compute_bounds(&asm.components);
    asm.notes = build_notes(&params, &cf, &rules);
    asm
}

/// 在指定位置施一道横栱，并在其两端置散斗、中心置齐心斗。
fn add_henggong_and_dou(
    comps: &mut Vec<Component>,
    push: &mut impl FnMut(
        &mut Vec<Component>,
        String,
        ComponentKind,
        u8,
        Vec3,
        Transform,
        Mesh,
    ),
    rules: &Rules,
    z: f32,
    cx: f32,
    gong_len: f32,
    name: &str,
    layer: u8,
) {
    let mesh = components::build_henggong(gong_len, rules);
    push(
        comps,
        name.to_string(),
        ComponentKind::HengGong,
        layer,
        [1.0, 0.0, 0.35],
        Transform {
            translation: [cx, 0.0, z],
            ..Default::default()
        },
        mesh,
    );
    let dou_z = z + rules.dan_height - 2.0;
    let half = gong_len * 0.5;
    for (sx, sname) in [(-1.0, "西散斗"), (1.0, "东散斗")] {
        let d = components::build_dou(
            rules.xiaodou_length,
            rules.xiaodou_width,
            rules.xiaodou_height,
            rules,
        );
        push(
            comps,
            format!("{}{}", name, sname),
            ComponentKind::XiaoDou,
            layer,
            [sx, 0.0, 0.6],
            Transform {
                translation: [cx + sx * half, 0.0, dou_z],
                ..Default::default()
            },
            d,
        );
    }
    let qx = components::build_dou(
        rules.xiaodou_length,
        rules.xiaodou_width,
        rules.xiaodou_height,
        rules,
    );
    push(
        comps,
        format!("{}齐心斗", name),
        ComponentKind::XiaoDou,
        layer,
        [0.0, 0.0, 0.8],
        Transform {
            translation: [cx, 0.0, dou_z],
            ..Default::default()
        },
        qx,
    );
}

fn scale_mesh(m: &Mesh, f: f32) -> Mesh {
    Mesh {
        positions: m.positions.iter().map(|v| v * f).collect(),
        normals: m.normals.clone(),
        material_ids: m.material_ids.clone(),
        indices: m.indices.clone(),
    }
}

fn compute_bounds(comps: &[Component]) -> Aabb {
    let mut b = Aabb::default();
    for c in comps {
        for p in c.mesh.positions.chunks_exact(3) {
            let w = apply_transform(&c.transform, [p[0], p[1], p[2]]);
            for k in 0..3 {
                b.min[k] = b.min[k].min(w[k]);
                b.max[k] = b.max[k].max(w[k]);
            }
        }
    }
    b
}

fn materials(style: MaterialStyle) -> Vec<MaterialSlot> {
    let painted = style == MaterialStyle::Caihua;
    vec![
        MaterialSlot { id: 0, name: "木面".into(), base_color: [0.62, 0.45, 0.28], painted },
        MaterialSlot { id: 1, name: "斗耳".into(), base_color: if painted { [0.72, 0.18, 0.14] } else { [0.66, 0.49, 0.31] }, painted },
        MaterialSlot { id: 2, name: "斗平".into(), base_color: if painted { [0.82, 0.55, 0.16] } else { [0.60, 0.43, 0.26] }, painted },
        MaterialSlot { id: 3, name: "斗欹".into(), base_color: if painted { [0.55, 0.12, 0.10] } else { [0.55, 0.39, 0.23] }, painted },
        MaterialSlot { id: 4, name: "彩画端".into(), base_color: if painted { [0.78, 0.22, 0.16] } else { [0.64, 0.47, 0.30] }, painted },
    ]
}

fn build_notes(params: &AssemblyParams, cf: &CaiFen, rules: &Rules) -> Vec<String> {
    let mut n = Vec::new();
    n.push(format!(
        "采用{}，{}：1 分 = {:.1} mm（按 1 宋尺 = 312 mm 推算），材广 15 分 = {:.1} mm，足材广 21 分。",
        params.bracket_set.label(),
        CaiGrade::from_number(params.cai_grade).unwrap().label(),
        cf.fen_m * 1000.0,
        cf.cai_height_m * 1000.0
    ));
    n.push("栌斗 32×32×20 分，散斗/交互斗 16×14×10 分，斗分耳、平、欹三段，比例 2:1:2，依《营造法式·卷四》。".into());
    n.push(format!(
        "每跳出跳 {} 分；华栱足材 10×21 分，横栱单材 10×15 分；令栱 72 分、瓜栱 62 分、万栱 92 分。",
        rules.jump
    ));
    n.push(format!(
        "昂制：{}。法式未规定昂身死数，本模型取降 {} 分/平 {} 分（约 {:.0}°），昂尾长 {} 分为判断值。",
        match params.ang_style {
            AngStyle::XiaAng => "下昂",
            AngStyle::ShangAng => "上昂",
            AngStyle::None => "卷头造（不用昂）",
        },
        rules.ang_slope_rise, rules.ang_slope_run,
        rules.ang_angle().abs().to_degrees(),
        rules.ang_tail_length
    ));
    n.push("栱端卷杀取 4 瓣、每瓣收 1 分；耍头蚂蚱头削斜段长 12 分，均为法式精神下的参数化判断值。".into());
    n.push("榫卯表达：构件底嵌入斗耳上口 2 分模拟榫头入斗口，未做布尔开槽，保证实时重建性能。".into());
    n
}
