//! glTF 2.0 二进制（.glb）导出。
//!
//! - 每个构件一个 node + mesh，保留装配 TRS；
//! - 同一构件按材质编号拆成多个 primitive，材质走标准 PBR，
//!   木纹/彩画的程序化纹理是前端渲染特性，导出文件用对应基色表达；
//! - 内部 Z-up → glTF Y-up 由根节点 -90° 旋转完成。

use crate::model::Assembly;
use serde::Serialize;
use serde_json::json;

const GLTF_UNSIGNED_INT: u32 = 5125;
const GLTF_FLOAT: u32 = 5126;
const GLTF_ARRAY_BUFFER: u32 = 34962;
const GLTF_ELEMENT_ARRAY_BUFFER: u32 = 34963;

pub fn export_glb(asm: &Assembly) -> Vec<u8> {
    let mut bin: Vec<u8> = Vec::new();
    let mut buffer_views = Vec::new();
    let mut accessors = Vec::new();
    let mut meshes = Vec::new();
    let mut nodes = Vec::new();

    // 闭包返回的访问器索引（bufferView 索引在闭包内已写入 JSON）。
    struct ViewRef {
        acc: usize,
    }

    let align4 = |bin: &mut Vec<u8>| {
        while bin.len() % 4 != 0 {
            bin.push(0);
        }
    };

    let push_f32_view = |bin: &mut Vec<u8>,
                             buffer_views: &mut Vec<serde_json::Value>,
                             accessors: &mut Vec<serde_json::Value>,
                             data: &[f32],
                             target: u32,
                             component_type: u32,
                             ttype: &str,
                             min: Option<&[f32]>,
                             max: Option<&[f32]>|
     -> ViewRef {
        align4(bin);
        let offset = bin.len();
        for v in data {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let view = buffer_views.len();
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": data.len() * 4,
            "target": target
        }));
        let count = if ttype == "SCALAR" { data.len() } else { data.len() / 3 };
        let acc = accessors.len();
        let mut a = json!({
            "bufferView": view,
            "componentType": component_type,
            "count": count,
            "type": ttype
        });
        if let (Some(lo), Some(hi)) = (min, max) {
            a["min"] = json!(lo);
            a["max"] = json!(hi);
        }
        accessors.push(a);
        ViewRef { acc }
    };

    let push_u32_view = |bin: &mut Vec<u8>,
                             buffer_views: &mut Vec<serde_json::Value>,
                             accessors: &mut Vec<serde_json::Value>,
                             data: &[u32]|
     -> ViewRef {
        align4(bin);
        let offset = bin.len();
        for v in data {
            bin.extend_from_slice(&v.to_le_bytes());
        }
        let view = buffer_views.len();
        buffer_views.push(json!({
            "buffer": 0,
            "byteOffset": offset,
            "byteLength": data.len() * 4,
            "target": GLTF_ELEMENT_ARRAY_BUFFER
        }));
        let acc = accessors.len();
        accessors.push(json!({
            "bufferView": view,
            "componentType": GLTF_UNSIGNED_INT,
            "count": data.len(),
            "type": "SCALAR"
        }));
        ViewRef { acc }
    };

    for c in &asm.components {
        // 按材质编号分组，组内重排索引。
        let groups = 5u16;
        let mut group_verts: Vec<Vec<usize>> = vec![Vec::new(); groups as usize];
        for vi in 0..c.mesh.vertex_count() {
            let m = c.mesh.material_ids[vi] as usize;
            group_verts[m].push(vi);
        }

        let mut primitives = Vec::new();
        for (mat_id, verts) in group_verts.iter().enumerate() {
            if verts.is_empty() {
                continue;
            }
            let mut remap = vec![u32::MAX; c.mesh.vertex_count()];
            let mut positions = Vec::with_capacity(verts.len() * 3);
            let mut normals = Vec::with_capacity(verts.len() * 3);
            let mut min = [f32::INFINITY; 3];
            let mut max = [f32::NEG_INFINITY; 3];
            for (new_idx, &old) in verts.iter().enumerate() {
                remap[old] = new_idx as u32;
                let p = &c.mesh.positions[old * 3..old * 3 + 3];
                let n = &c.mesh.normals[old * 3..old * 3 + 3];
                positions.extend_from_slice(p);
                normals.extend_from_slice(n);
                for k in 0..3 {
                    min[k] = min[k].min(p[k]);
                    max[k] = max[k].max(p[k]);
                }
            }
            let mut indices = Vec::new();
            for tri in c.mesh.indices.chunks_exact(3) {
                if remap[tri[0] as usize] != u32::MAX {
                    indices.extend([remap[tri[0] as usize], remap[tri[1] as usize], remap[tri[2] as usize]]);
                }
            }

            let pos_ref = push_f32_view(
                &mut bin, &mut buffer_views, &mut accessors,
                &positions, GLTF_ARRAY_BUFFER, GLTF_FLOAT, "VEC3",
                Some(&min), Some(&max),
            );
            let nrm_ref = push_f32_view(
                &mut bin, &mut buffer_views, &mut accessors,
                &normals, GLTF_ARRAY_BUFFER, GLTF_FLOAT, "VEC3", None, None,
            );
            let idx_ref = push_u32_view(&mut bin, &mut buffer_views, &mut accessors, &indices);

            primitives.push(json!({
                "attributes": { "POSITION": pos_ref.acc, "NORMAL": nrm_ref.acc },
                "indices": idx_ref.acc,
                "material": mat_id
            }));
        }

        let mesh_idx = meshes.len();
        meshes.push(json!({ "name": c.name, "primitives": primitives }));

        let t = &c.transform;
        let (s, q) = (t.rotation_z.sin(), t.rotation_z.cos());
        // 绕 Z 旋转的四元数。
        nodes.push(json!({
            "name": c.name,
            "mesh": mesh_idx,
            "translation": t.translation,
            "rotation": [0.0, 0.0, s * 0.5_f32.sin(), q],
            "extras": {
                "componentKind": format!("{:?}", c.kind),
                "layer": c.layer,
                "explodeDir": c.explode_dir
            }
        }));
    }

    // 材质：PBR 基色 + 粗糙度。
    let materials: Vec<_> = asm
        .materials
        .iter()
        .map(|m| {
            json!({
                "name": m.name,
                "pbrMetallicRoughness": {
                    "baseColorFactor": [m.base_color[0], m.base_color[1], m.base_color[2], 1.0],
                    "metallicFactor": 0.0,
                    "roughnessFactor": 0.72
                },
                "doubleSided": false
            })
        })
        .collect();

    // 根节点：Z-up → Y-up（绕 X 轴 -90°，四元数 x = sin(-45°)）。
    let root_idx = nodes.len();
    let child_indices: Vec<_> = (0..asm.components.len()).collect();
    nodes.push(json!({
        "name": "dougong_root_ZupToYup",
        "rotation": [-(std::f32::consts::FRAC_PI_4).sin(), 0.0, 0.0, std::f32::consts::FRAC_PI_4.cos()],
        "children": child_indices
    }));

    let gltf = json!({
        "asset": { "version": "2.0", "generator": "dougong-core (Yingzao Fashi parametric)" },
        "scene": 0,
        "scenes": [{ "name": "puzuo", "nodes": [root_idx] }],
        "nodes": nodes,
        "meshes": meshes,
        "materials": materials,
        "accessors": accessors,
        "bufferViews": buffer_views,
        "buffers": [{ "byteLength": bin.len() }]
    });

    let mut json_bytes = serde_json::to_vec(&gltf).expect("gltf json");
    while json_bytes.len() % 4 != 0 {
        json_bytes.push(b' ');
    }
    while bin.len() % 4 != 0 {
        bin.push(0);
    }

    let total = 12 + 8 + json_bytes.len() + 8 + bin.len();
    let mut out = Vec::with_capacity(total);
    out.extend_from_slice(b"glTF");
    out.extend_from_slice(&2u32.to_le_bytes());
    out.extend_from_slice(&(total as u32).to_le_bytes());
    out.extend_from_slice(&(json_bytes.len() as u32).to_le_bytes());
    out.extend_from_slice(b"JSON");
    out.extend_from_slice(&json_bytes);
    out.extend_from_slice(&(bin.len() as u32).to_le_bytes());
    out.extend_from_slice(b"BIN\0");
    out.extend_from_slice(&bin);
    out
}

#[allow(dead_code)]
fn _unused(_: impl Serialize) {}
