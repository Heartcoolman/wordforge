//! 哈欠检测模块
//!
//! 基于 MAR (Mouth Aspect Ratio) 进行哈欠检测。
//! MAR = (|p2-p8| + |p3-p7| + |p4-p6|) / (2 * |p1-p5|)
//!
//! 哈欠的特征：
//! - MAR 超过阈值 (0.6)
//! - 持续时间在 2-8 秒之间
//! - 哈欠频率增加是疲劳的重要信号

use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

/// 二维点
#[derive(Clone, Copy)]
struct Point {
    x: f64,
    y: f64,
}

impl Point {
    fn new(x: f64, y: f64) -> Self {
        Self { x, y }
    }

    fn distance(&self, other: &Point) -> f64 {
        ((self.x - other.x).powi(2) + (self.y - other.y).powi(2)).sqrt()
    }
}

/// 哈欠事件记录
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct YawnEvent {
    /// 哈欠完成的时间戳（毫秒）
    timestamp: f64,
    /// 哈欠持续时间（毫秒）
    duration: f64,
}

/// 哈欠检测结果
#[wasm_bindgen]
pub struct YawnResult {
    /// 当前 MAR 值
    pub mar: f64,
    /// 是否正在打哈欠
    pub is_yawning: bool,
    /// 是否刚完成一次哈欠
    pub yawn_detected: bool,
    /// 近期哈欠频率（次/分钟）
    pub yawn_rate: f64,
    /// 近期哈欠次数（窗口内）
    pub yawn_count: u32,
}

/// 哈欠检测器
///
/// 基于 MAR 值检测哈欠事件。
/// 有效哈欠需要 MAR 超过阈值并持续 2-8 秒。
#[wasm_bindgen]
pub struct YawnDetector {
    /// MAR 阈值
    mar_threshold: f64,
    /// 最小哈欠持续时间（毫秒）
    min_duration_ms: f64,
    /// 最大哈欠持续时间（毫秒）
    max_duration_ms: f64,
    /// 是否正在打哈欠
    is_yawning: bool,
    /// 哈欠开始时间戳
    yawn_start_ts: f64,
    /// 哈欠事件历史
    yawn_history: VecDeque<YawnEvent>,
    /// 频率计算窗口大小（毫秒），默认5分钟
    window_ms: f64,
}

#[wasm_bindgen]
impl YawnDetector {
    /// 创建新的哈欠检测器
    ///
    /// # 参数
    /// - `mar_threshold`: MAR 阈值，推荐 0.6
    #[wasm_bindgen(constructor)]
    pub fn new(mar_threshold: f64) -> Self {
        Self {
            mar_threshold,
            min_duration_ms: 2000.0, // 2秒
            max_duration_ms: 8000.0, // 8秒
            is_yawning: false,
            yawn_start_ts: 0.0,
            yawn_history: VecDeque::new(),
            window_ms: 300_000.0, // 5分钟窗口
        }
    }

    /// 计算 MAR 并更新检测状态
    ///
    /// 输入: 16个浮点数 (8个嘴部关键点 × 2个坐标)
    /// 点排列：p1(x,y), p2(x,y), ..., p8(x,y)
    /// - p1, p5: 嘴角点（水平方向）
    /// - p2, p8: 上下唇外侧点（垂直配对1）
    /// - p3, p7: 上下唇内侧点（垂直配对2）
    /// - p4, p6: 上下唇中心点（垂直配对3）
    ///
    /// # 参数
    /// - `landmarks`: 嘴部关键点坐标数组
    /// - `timestamp`: 当前时间戳（毫秒）
    pub fn update(&mut self, landmarks: &[f64], timestamp: f64) -> YawnResult {
        let mar = self.calculate_mar(landmarks);
        let mut yawn_detected = false;

        if mar >= self.mar_threshold {
            if !self.is_yawning {
                // 开始打哈欠
                self.is_yawning = true;
                self.yawn_start_ts = timestamp;
            }
        } else if self.is_yawning {
            // 哈欠结束
            self.is_yawning = false;
            let duration = timestamp - self.yawn_start_ts;

            // 检查持续时间是否在有效范围内
            if duration >= self.min_duration_ms && duration <= self.max_duration_ms {
                yawn_detected = true;
                self.yawn_history.push_back(YawnEvent {
                    timestamp,
                    duration,
                });
            }
        }

        // 清理窗口外的旧事件
        let cutoff = timestamp - self.window_ms;
        while let Some(front) = self.yawn_history.front() {
            if front.timestamp < cutoff {
                self.yawn_history.pop_front();
            } else {
                break;
            }
        }

        let yawn_count = self.yawn_history.len() as u32;
        let yawn_rate = self.calculate_yawn_rate(timestamp);

        YawnResult {
            mar,
            is_yawning: self.is_yawning,
            yawn_detected,
            yawn_rate,
            yawn_count,
        }
    }

    /// 仅计算 MAR 值（不更新状态）
    ///
    /// 输入格式同 update 方法
    #[wasm_bindgen(js_name = "calculateMAR")]
    pub fn calculate_mar(&self, landmarks: &[f64]) -> f64 {
        if landmarks.len() < 16 {
            return 0.0;
        }

        let p1 = Point::new(landmarks[0], landmarks[1]);
        let p2 = Point::new(landmarks[2], landmarks[3]);
        let p3 = Point::new(landmarks[4], landmarks[5]);
        let p4 = Point::new(landmarks[6], landmarks[7]);
        let p5 = Point::new(landmarks[8], landmarks[9]);
        let p6 = Point::new(landmarks[10], landmarks[11]);
        let p7 = Point::new(landmarks[12], landmarks[13]);
        let p8 = Point::new(landmarks[14], landmarks[15]);

        // MAR = (|p2-p8| + |p3-p7| + |p4-p6|) / (2 * |p1-p5|)
        let horizontal = p1.distance(&p5);
        if horizontal < 1e-6 {
            return 0.0;
        }

        let v1 = p2.distance(&p8);
        let v2 = p3.distance(&p7);
        let v3 = p4.distance(&p6);

        (v1 + v2 + v3) / (2.0 * horizontal)
    }

    /// 获取近期哈欠次数
    #[wasm_bindgen(js_name = "getYawnCount")]
    pub fn get_yawn_count(&self) -> u32 {
        self.yawn_history.len() as u32
    }

    /// 获取哈欠频率（次/分钟）
    #[wasm_bindgen(js_name = "getYawnRate")]
    pub fn get_yawn_rate(&self) -> f64 {
        if let Some(last) = self.yawn_history.back() {
            self.calculate_yawn_rate(last.timestamp)
        } else {
            0.0
        }
    }

    /// 设置 MAR 阈值
    #[wasm_bindgen(js_name = "setThreshold")]
    pub fn set_threshold(&mut self, threshold: f64) {
        self.mar_threshold = threshold;
    }

    /// 设置有效哈欠持续时间范围（秒）
    #[wasm_bindgen(js_name = "setDurationRange")]
    pub fn set_duration_range(&mut self, min_seconds: f64, max_seconds: f64) {
        self.min_duration_ms = min_seconds * 1000.0;
        self.max_duration_ms = max_seconds * 1000.0;
    }

    /// 重置检测器状态
    pub fn reset(&mut self) {
        self.is_yawning = false;
        self.yawn_start_ts = 0.0;
        self.yawn_history.clear();
    }
}

impl YawnDetector {
    /// 计算哈欠频率（次/分钟）
    fn calculate_yawn_rate(&self, current_ts: f64) -> f64 {
        if self.yawn_history.is_empty() {
            return 0.0;
        }

        let first_ts = self.yawn_history.front().unwrap().timestamp;
        let elapsed_ms = current_ts - first_ts;

        if elapsed_ms < 1000.0 {
            return 0.0;
        }

        let count = self.yawn_history.len() as f64;
        count / (elapsed_ms / 60_000.0)
    }
}
