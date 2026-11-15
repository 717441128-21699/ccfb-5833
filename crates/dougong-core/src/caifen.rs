//! 《营造法式》材分制。
//!
//! # 制度依据
//! - 材广（高）15 分、厚（宽）10 分；栔广 6 分、厚 4 分。
//!   单材广 15 分，足材 = 一材一栔，广 21 分。
//! - 八等材的实际尺寸按《营造法式·卷四·材》所记宋尺数值，
//!   本库取 1 宋尺 = 0.312 m 换算（学界常用 0.31–0.32 m，取 0.312 m，见 docs/DESIGN.md）。
//! - 构件分件名（栌斗、散斗、华栱、昂、耍头等）的分°尺寸见 [`rules`]，
//!   均以「分」为单位，与材等无关；材等只决定 1 分的实际毫米数。

use serde::{Deserialize, Serialize};

/// 一等材至八等材。表中广/厚单位为宋寸（《法式》原文）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaiGrade {
    Yi = 1,
    Er = 2,
    San = 3,
    Si = 4,
    Wu = 5,
    Liu = 6,
    Qi = 7,
    Ba = 8,
}

impl CaiGrade {
    pub fn from_number(n: u8) -> Option<Self> {
        Some(match n {
            1 => CaiGrade::Yi,
            2 => CaiGrade::Er,
            3 => CaiGrade::San,
            4 => CaiGrade::Si,
            5 => CaiGrade::Wu,
            6 => CaiGrade::Liu,
            7 => CaiGrade::Qi,
            8 => CaiGrade::Ba,
            _ => return None,
        })
    }

    /// (材广宋寸, 材厚宋寸)，出自《法式·卷四》。
    fn song_cun(self) -> (f32, f32) {
        match self {
            CaiGrade::Yi => (9.0, 6.0),
            CaiGrade::Er => (8.25, 5.5),
            CaiGrade::San => (7.5, 5.0),
            CaiGrade::Si => (7.2, 4.8),
            CaiGrade::Wu => (6.6, 4.4),
            CaiGrade::Liu => (6.0, 4.0),
            CaiGrade::Qi => (5.25, 3.5),
            CaiGrade::Ba => (4.5, 3.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            CaiGrade::Yi => "一等材",
            CaiGrade::Er => "二等材",
            CaiGrade::San => "三等材",
            CaiGrade::Si => "四等材",
            CaiGrade::Wu => "五等材",
            CaiGrade::Liu => "六等材",
            CaiGrade::Qi => "七等材",
            CaiGrade::Ba => "八等材",
        }
    }
}

const SONG_CHI_M: f32 = 0.312;

/// 材分制下的尺度环境，所有构件以「分」描述，经此换算到米。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct CaiFen {
    pub grade: u8,
    /// 1 分的实际长度（米）。
    pub fen_m: f32,
    /// 材广（米），恒为 15 分。
    pub cai_height_m: f32,
    /// 材厚（米），恒为 10 分。
    pub cai_width_m: f32,
    /// 栔广（米），6 分。
    pub qi_height_m: f32,
    /// 足材广（米），21 分。
    pub zu_cai_height_m: f32,
}

impl CaiFen {
    pub fn new(grade: CaiGrade) -> Self {
        let (height_cun, width_cun) = grade.song_cun();
        let cai_height_m = height_cun / 10.0 * SONG_CHI_M;
        let cai_width_m = width_cun / 10.0 * SONG_CHI_M;
        // 材广 15 分，故 1 分 = 材广 / 15。
        let fen_m = cai_height_m / 15.0;
        Self {
            grade: grade as u8,
            fen_m,
            cai_height_m,
            cai_width_m,
            qi_height_m: 6.0 * fen_m,
            zu_cai_height_m: 21.0 * fen_m,
        }
    }

    /// 分 → 米。
    pub fn m(&self, fen: f32) -> f32 {
        fen * self.fen_m
    }
}

/// 单材/足材选择（华栱、昂等受力构件用足材，横栱用单材）。
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum CaiSection {
    Dan,
    Zu,
}

impl CaiSection {
    /// 构件截面（分）：(厚=宽 10 分, 高)。
    pub fn fen_size(self) -> (f32, f32) {
        match self {
            CaiSection::Dan => (10.0, 15.0),
            CaiSection::Zu => (10.0, 21.0),
        }
    }
}

/// 法式分°尺寸规则集合（单位均为分）。数值来源与判断说明见 docs/DESIGN.md。
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct Rules {
    // —— 斗 ——
    /// 栌斗长（面阔方向底面）。
    pub ludou_length: f32,
    /// 栌斗底面宽（进深方向）。
    pub ludou_width: f32,
    /// 栌斗总高。
    pub ludou_height: f32,
    /// 斗之耳:平:欹 = 2:1:2，欹底开口按斗长的 0.8 收分。
    pub dou_ear_ratio: f32,
    pub dou_ping_ratio: f32,
    pub dou_qi_ratio: f32,
    pub dou_taper: f32,

    // —— 散斗/交互斗/齐心斗 ——
    pub xiaodou_length: f32,
    pub xiaodou_width: f32,
    pub xiaodou_height: f32,

    // —— 栱 ——
    /// 华栱/横栱材厚。
    pub gong_thickness: f32,
    /// 单材广。
    pub dan_height: f32,
    /// 足材广。
    pub zu_height: f32,
    /// 标准栱长（令栱 72 分）。
    pub linggong_length: f32,
    /// 瓜栱长 62 分。
    pub guagong_length: f32,
    /// 万栱长 92 分。
    pub wanggong_length: f32,
    /// 华栱一跳出跳 30 分（《法式》：每跳之长不过 30 分）。
    pub tiao_length: f32,
    /// 栱端卷杀瓣数与每瓣收分（分）。
    pub juansha_bans: u8,
    pub juansha_per_ban: f32,

    // —— 昂 ——
    /// 昂自斗心向外斜下，斜率取 2/5（即降下 2 分 / 水平 5 分，
    /// 《法式》未给死数值，取宋构常用约 22° 的折中，见 DESIGN.md）。
    pub ang_slope_rise: f32,
    pub ang_slope_run: f32,
    /// 昂尾（挑斡/上伸部分）向内水平长度（分）。
    pub ang_tail_length: f32,

    // —— 耍头 ——
    /// 耍头伸出令栱之外的长度。
    pub shuatou_overhang: f32,
    /// 蚂蚱头端部高度（足材 21 分基础上削尖到该高度）。
    pub mazha_tip_height: f32,

    // —— 装配 ——
    /// 每跳出跳（分）。
    pub jump: f32,
}

impl Rules {
    /// 《营造法式》制度下的标准分°取值。
    pub fn yingzao_fashi() -> Self {
        Self {
            ludou_length: 32.0,
            ludou_width: 32.0,
            ludou_height: 20.0,
            dou_ear_ratio: 0.4,
            dou_ping_ratio: 0.2,
            dou_qi_ratio: 0.4,
            dou_taper: 0.8,

            xiaodou_length: 16.0,
            xiaodou_width: 14.0,
            xiaodou_height: 10.0,

            gong_thickness: 10.0,
            dan_height: 15.0,
            zu_height: 21.0,
            linggong_length: 72.0,
            guagong_length: 62.0,
            wanggong_length: 92.0,
            tiao_length: 30.0,
            juansha_bans: 4,
            juansha_per_ban: 1.0,

            ang_slope_rise: 2.0,
            ang_slope_run: 5.0,
            ang_tail_length: 40.0,

            shuatou_overhang: 24.0,
            mazha_tip_height: 8.0,

            jump: 30.0,
        }
    }

    /// 昂的倾角（弧度，负值表示向外斜下）。
    pub fn ang_angle(&self) -> f32 {
        -(self.ang_slope_rise / self.ang_slope_run).atan()
    }
}
