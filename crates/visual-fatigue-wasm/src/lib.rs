//! 视觉疲劳检测 WASM 库
//!
//! 本库提供基于面部关键点的实时视觉疲劳检测算法，编译为 WebAssembly
//! 在浏览器端高性能运行。
//!
//! ## 模块
//! - `ear`: EAR (Eye Aspect Ratio) 眼部纵横比计算
//! - `perclos`: PERCLOS 闭眼时间百分比统计
//! - `blink`: 眨眼检测状态机
//! - `yawn`: 哈欠检测 (MAR)
//! - `head_pose`: 头部姿态估计
//! - `fatigue`: 综合疲劳评分

pub mod blink;
pub mod ear;
pub mod fatigue;
pub mod head_pose;
pub mod perclos;
pub mod yawn;

// 重新导出核心类型，方便外部使用
pub use blink::BlinkDetector;
pub use ear::EARCalculator;
pub use fatigue::FatigueScorer;
pub use head_pose::HeadPoseEstimator;
pub use perclos::PERCLOSCalculator;
pub use yawn::YawnDetector;

/// 二维点，表示一个关键点的坐标
#[derive(Clone, Copy)]
pub(crate) struct Point {
    pub x: f64,
    pub y: f64,
}

impl Point {
    pub fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    /// 计算两点之间的欧几里得距离
    pub fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}
