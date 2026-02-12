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
