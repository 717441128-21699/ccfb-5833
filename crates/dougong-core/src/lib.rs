//! dougong-core：参数化斗拱建模核心（纯 Rust，可编译为 WASM）。
//!
//! 数据流：[`model::AssemblyParams`] → [`assembly::build_assembly`] →
//! [`model::Assembly`]（网格 + 装配 + 材质 + 说明）→ JSON 供前端 /
//! [`gltf::export_glb`] 导出标准 glTF。

pub mod assembly;
pub mod caifen;
pub mod components;
pub mod geom;
pub mod gltf;
pub mod mesh_builder;
pub mod model;

use model::{Assembly, AssemblyParams};

/// 主入口：参数 → 完整装配。
pub fn generate(params: &AssemblyParams) -> Assembly {
    assembly::build_assembly(params)
}

/// 主入口：参数 → JSON 字节（WASM 边界与前端共用）。
pub fn generate_json(params_json: &str) -> Result<String, String> {
    let params: AssemblyParams =
        serde_json::from_str(params_json).map_err(|e| format!("参数解析失败: {e}"))?;
    let asm = generate(&params);
    serde_json::to_string(&asm).map_err(|e| format!("序列化失败: {e}"))
}

/// 参数 → GLB 字节。
pub fn generate_glb(params_json: &str) -> Result<Vec<u8>, String> {
    let params: AssemblyParams =
        serde_json::from_str(params_json).map_err(|e| format!("参数解析失败: {e}"))?;
    Ok(gltf::export_glb(&generate(&params)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::caifen::CaiGrade;
    use crate::model::{AngStyle, BracketSetType, MaterialStyle};

    fn params(ty: BracketSetType) -> AssemblyParams {
        AssemblyParams {
            bracket_set: ty,
            cai_grade: 5,
            ang_style: AngStyle::XiaAng,
            material_style: MaterialStyle::Wood,
            include_inner_jumps: true,
        }
    }

    #[test]
    fn fen_scale_matches_fashi() {
        let cf = caifen::CaiFen::new(CaiGrade::Yi);
        // 一等材广 9 宋寸 = 0.2808 m，15 分 → 每分 18.72 mm。
        assert!((cf.fen_m * 1000.0 - 18.72).abs() < 1e-4);
        assert!((cf.cai_width_m - 0.6 * 0.312).abs() < 1e-6);
        let cf8 = caifen::CaiFen::new(CaiGrade::Ba);
        // 八等材广 4.5 寸 → 每分 9.36 mm。
        assert!((cf8.fen_m * 1000.0 - 9.36).abs() < 1e-4);
    }

    #[test]
    fn all_bracket_sets_build() {
        for ty in [
            BracketSetType::DoukouTiao,
            BracketSetType::SiPuZuo,
            BracketSetType::WuPuZuo,
            BracketSetType::LiuPuZuo,
            BracketSetType::QiPuZuo,
            BracketSetType::BaPuZuo,
        ] {
            let asm = generate(&params(ty));
            assert!(!asm.components.is_empty(), "{:?} 无构件", ty);
            // 网格健全性：索引不越界、法线为单位向量、三角形数量 > 0。
            for c in &asm.components {
                let nv = c.mesh.vertex_count();
                assert!(nv > 0);
                assert!(c.mesh.indices.iter().all(|&i| i as usize >= nv) == false);
                assert_eq!(c.mesh.indices.len() % 3, 0);
                for n in c.mesh.normals.chunks_exact(3) {
                    let l = (n[0].powi(2) + n[1].powi(2) + n[2].powi(2)).sqrt();
                    assert!((l - 1.0).abs() < 1e-4, "法线未归一化: {l}");
                }
                assert!(c.mesh.material_ids.iter().all(|&m| m < 5));
            }
            assert!(asm.bounds.max[2] > asm.bounds.min[2]);
        }
    }

    #[test]
    fn layer_counts_increase_with_jumps() {
        let a4 = generate(&params(BracketSetType::SiPuZuo));
        let a7 = generate(&params(BracketSetType::QiPuZuo));
        assert!(a7.components.len() > a4.components.len());
        let max_layer = |a: &Assembly| a.components.iter().map(|c| c.layer).max().unwrap();
        // 四铺作外跳 1 跳 + 耍头层 → 最大层号 2；七铺作 4 跳 → 5。
        assert_eq!(max_layer(&a4), 2);
        assert_eq!(max_layer(&a7), 5);
    }

    #[test]
    fn ang_styles_and_half_assembly() {
        for style in [AngStyle::XiaAng, AngStyle::ShangAng, AngStyle::None] {
            let mut p = params(BracketSetType::WuPuZuo);
            p.ang_style = style;
            let asm = generate(&p);
            let has_ang = asm.components.iter().any(|c| c.kind == model::ComponentKind::Ang);
            assert_eq!(has_ang, style != AngStyle::None);
        }
        let mut half = params(BracketSetType::LiuPuZuo);
        half.include_inner_jumps = false;
        let full = generate(&half);
        let mut both = half.clone();
        both.include_inner_jumps = true;
        let full_both = generate(&both);
        assert!(full_both.components.len() > full.components.len());
    }

    #[test]
    fn glb_is_valid_container() {
        let asm = generate(&params(BracketSetType::WuPuZuo));
        let glb = gltf::export_glb(&asm);
        assert_eq!(&glb[0..4], b"glTF");
        assert_eq!(u32::from_le_bytes(glb[4..8].try_into().unwrap()), 2);
        let total = u32::from_le_bytes(glb[8..12].try_into().unwrap()) as usize;
        assert_eq!(total, glb.len());
        // 两个 chunk 头。
        let json_len = u32::from_le_bytes(glb[12..16].try_into().unwrap()) as usize;
        assert_eq!(&glb[16..20], b"JSON");
        let json = &glb[20..20 + json_len];
        let v: serde_json::Value = serde_json::from_slice(json).unwrap();
        assert_eq!(v["asset"]["version"], "2.0");
        assert!(v["meshes"].as_array().unwrap().len() == asm.components.len());
        // BIN chunk 头位置 4 字节对齐。
        let bin_head = 20 + json_len;
        assert_eq!(&glb[bin_head + 4..bin_head + 8], b"BIN\0");
    }

    #[test]
    fn json_roundtrip_and_rebuild_time_budget() {
        let p = params(BracketSetType::QiPuZuo);
        let s = generate_json(&serde_json::to_string(&p).unwrap()).unwrap();
        let back: Assembly = serde_json::from_str(&s).unwrap();
        assert!(!back.components.is_empty());
        // 重建耗时预算：最大朵（八铺作）也应远低于 1 秒。
        let start = std::time::Instant::now();
        for _ in 0..100 {
            std::hint::black_box(generate(&params(BracketSetType::BaPuZuo)));
        }
        let per = start.elapsed().as_secs_f64() / 100.0;
        assert!(per < 0.01, "单次重建 {per:.4}s 超出预算");
    }
}
