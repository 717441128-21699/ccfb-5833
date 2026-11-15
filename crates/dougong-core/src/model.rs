//! 用户输入参数、构件清单与装配结果的数据结构。

use crate::geom::{Aabb, Mesh, Transform};
use serde::{Deserialize, Serialize};

/// 铺作类型（外跳跳数决定名称；本模型内外对称，里跳跳数同外跳）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum BracketSetType {
    /// 斗口跳（无出跳华栱，仅栌斗上置横栱，作为最简参照）。
    #[serde(alias = "doukou")]
    DoukouTiao,
    /// 四铺作：外跳一跳。
    #[serde(alias = "si")]
    SiPuZuo,
    /// 五铺作：外跳两跳。
    #[serde(alias = "wu")]
    WuPuZuo,
    /// 六铺作：外跳三跳。
    #[serde(alias = "liu")]
    LiuPuZuo,
    /// 七铺作：外跳四跳。
    #[serde(alias = "qi")]
    QiPuZuo,
    /// 八铺作：外跳五跳。
    #[serde(alias = "ba")]
    BaPuZuo,
}

impl BracketSetType {
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "doukou" => BracketSetType::DoukouTiao,
            "si" => BracketSetType::SiPuZuo,
            "wu" => BracketSetType::WuPuZuo,
            "liu" => BracketSetType::LiuPuZuo,
            "qi" => BracketSetType::QiPuZuo,
            "ba" => BracketSetType::BaPuZuo,
            _ => return None,
        })
    }

    pub fn id(self) -> &'static str {
        match self {
            BracketSetType::DoukouTiao => "doukou",
            BracketSetType::SiPuZuo => "si",
            BracketSetType::WuPuZuo => "wu",
            BracketSetType::LiuPuZuo => "liu",
            BracketSetType::QiPuZuo => "qi",
            BracketSetType::BaPuZuo => "ba",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            BracketSetType::DoukouTiao => "斗口跳",
            BracketSetType::SiPuZuo => "四铺作",
            BracketSetType::WuPuZuo => "五铺作",
            BracketSetType::LiuPuZuo => "六铺作",
            BracketSetType::QiPuZuo => "七铺作",
            BracketSetType::BaPuZuo => "八铺作",
        }
    }

    /// 外跳跳数（斗口跳为 0）。
    pub fn outer_jumps(self) -> u8 {
        match self {
            BracketSetType::DoukouTiao => 0,
            BracketSetType::SiPuZuo => 1,
            BracketSetType::WuPuZuo => 2,
            BracketSetType::LiuPuZuo => 3,
            BracketSetType::QiPuZuo => 4,
            BracketSetType::BaPuZuo => 5,
        }
    }
}

/// 昂的形制。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum AngStyle {
    /// 下昂：自斗心向外斜下（宋式铺作主流）。
    #[serde(alias = "xia")]
    XiaAng,
    /// 上昂：向内斜上，用于室内挑斡。
    #[serde(alias = "shang")]
    ShangAng,
    /// 不用昂，全部以华栱出跳（「卷头造」）。
    #[serde(alias = "none")]
    None,
}

impl AngStyle {
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "xia" => AngStyle::XiaAng,
            "shang" => AngStyle::ShangAng,
            "none" => AngStyle::None,
            _ => return None,
        })
    }
    pub fn id(self) -> &'static str {
        match self {
            AngStyle::XiaAng => "xia",
            AngStyle::ShangAng => "shang",
            AngStyle::None => "none",
        }
    }
}

/// 表面材质风格。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum MaterialStyle {
    /// 原木木纹。
    #[serde(alias = "wood")]
    Wood,
    /// 宋式彩画（丹粉刷饰，朱红为主、暖色叠晕）。
    #[serde(alias = "caihua")]
    Caihua,
}

impl MaterialStyle {
    pub fn from_id(id: &str) -> Option<Self> {
        Some(match id {
            "wood" => MaterialStyle::Wood,
            "caihua" => MaterialStyle::Caihua,
            _ => return None,
        })
    }
    pub fn id(self) -> &'static str {
        match self {
            MaterialStyle::Wood => "wood",
            MaterialStyle::Caihua => "caihua",
        }
    }
}

/// 用户输入的全部参数。serde 默认 camelCase，同时接受 snake_case 别名。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssemblyParams {
    #[serde(alias = "bracket_set")]
    pub bracket_set: BracketSetType,
    /// 材等 1–8。
    #[serde(alias = "cai_grade")]
    pub cai_grade: u8,
    /// 昂制。
    #[serde(alias = "ang_style")]
    pub ang_style: AngStyle,
    /// 材质风格。
    #[serde(alias = "material_style")]
    pub material_style: MaterialStyle,
    /// 是否生成里跳（室内一侧），关闭则只做外跳半朵，便于课堂观察。
    #[serde(alias = "include_inner_jumps")]
    pub include_inner_jumps: bool,
}

impl Default for AssemblyParams {
    fn default() -> Self {
        Self {
            bracket_set: BracketSetType::WuPuZuo,
            cai_grade: 5,
            ang_style: AngStyle::XiaAng,
            material_style: MaterialStyle::Wood,
            include_inner_jumps: true,
        }
    }
}

/// 构件大类，用于拆装动画分组与说明标注。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum ComponentKind {
    LuDou,
    HuaGong,
    HengGong,
    Ang,
    ShuaTou,
    XiaoDou,
}

impl ComponentKind {
    pub fn label(self) -> &'static str {
        match self {
            ComponentKind::LuDou => "栌斗",
            ComponentKind::HuaGong => "华栱",
            ComponentKind::HengGong => "横栱",
            ComponentKind::Ang => "昂",
            ComponentKind::ShuaTou => "耍头",
            ComponentKind::XiaoDou => "小斗",
        }
    }
}

/// 单个构件。网格为局部坐标，位置由 transform 给出。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Component {
    pub id: u32,
    pub name: String,
    pub kind: ComponentKind,
    /// 拆装层号：0 = 最底栌斗，数字越大越靠上；同层构件拆装时一起动作。
    pub layer: u8,
    /// 爆炸/拆装方向（单位向量，世界坐标），前端沿此方向平移。
    pub explode_dir: [f32; 3],
    pub transform: Transform,
    pub mesh: Mesh,
}

/// 材质槽（前端按 material_ids 索引）。
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MaterialSlot {
    pub id: u16,
    pub name: String,
    /// 示意基色（线性 RGB 0–1），前端程序化纹理以此为基调。
    pub base_color: [f32; 3],
    /// 彩画模式下是否画叠晕纹样。
    pub painted: bool,
}

/// 完整装配结果（前端与 glTF 导出的共同数据源）。
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Assembly {
    pub params: AssemblyParams,
    pub fen_m: f32,
    pub components: Vec<Component>,
    pub materials: Vec<MaterialSlot>,
    pub bounds: Aabb,
    /// 人类可读的推算说明（含法式依据与本模型的判断）。
    pub notes: Vec<String>,
}
