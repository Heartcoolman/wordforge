//! PERCLOS (Percentage of Eye Closure) 计算模块
//!
//! PERCLOS 是衡量疲劳程度的经典指标，表示在一段时间窗口内眼睛闭合的时间占比。
//! 使用60秒的滑动窗口，EAR 低于阈值时视为闭眼。
//! - PERCLOS < 0.15: 清醒
//! - PERCLOS 0.15 - 0.30: 轻度疲劳
//! - PERCLOS > 0.30: 明显疲劳

use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

/// 带时间戳的眼睛状态样本
#[derive(Clone, Copy)]
struct EyeSample {
    /// 是否闭眼
    is_closed: bool,
    /// 时间戳（毫秒）
    timestamp: f64,
}

/// PERCLOS 计算器
///
/// 基于滑动窗口统计闭眼时间占比。
/// 窗口大小默认60秒，通过时间戳自动管理窗口内的样本。
#[wasm_bindgen]
pub struct PERCLOSCalculator {
    /// EAR 阈值，低于此值视为闭眼
    ear_threshold: f64,
    /// 滑动窗口大小（毫秒）
    window_ms: f64,
    /// 样本队列
    samples: VecDeque<EyeSample>,
    /// 当前 PERCLOS 值
    current_perclos: f64,
}

#[wasm_bindgen]
impl PERCLOSCalculator {
    /// 创建新的 PERCLOS 计算器
    ///
    /// # 参数
    /// - `ear_threshold`: EAR 闭眼阈值，推荐 0.2
    /// - `window_seconds`: 滑动窗口大小（秒），推荐 60
    #[wasm_bindgen(constructor)]
    pub fn new(ear_threshold: f64, window_seconds: f64) -> Self {
        Self {
            ear_threshold,
            window_ms: window_seconds * 1000.0,
            samples: VecDeque::with_capacity(240),
            current_perclos: 0.0,
        }
    }

    /// 更新 PERCLOS，输入当前帧的 EAR 值和时间戳
    ///
    /// # 参数
    /// - `ear`: 当前帧的 EAR 值
    /// - `timestamp`: 当前时间戳（毫秒）
    ///
    /// # 返回
    /// 当前 PERCLOS 值 (0.0 - 1.0)
    pub fn update(&mut self, ear: f64, timestamp: f64) -> f64 {
        let is_closed = ear < self.ear_threshold;

        self.samples.push_back(EyeSample {
            is_closed,
            timestamp,
        });

        // 移除窗口外的旧样本
        let cutoff = timestamp - self.window_ms;
        while let Some(front) = self.samples.front() {
            if front.timestamp < cutoff {
                self.samples.pop_front();
            } else {
                break;
            }
        }

        // 计算 PERCLOS: 使用时间加权方式
        self.current_perclos = self.compute_perclos();
        self.current_perclos
    }

    /// 获取当前 PERCLOS 值
    #[wasm_bindgen(js_name = "getPERCLOS")]
    pub fn get_perclos(&self) -> f64 {
        self.current_perclos
    }

    /// 获取窗口内样本数量
    #[wasm_bindgen(js_name = "getSampleCount")]
    pub fn get_sample_count(&self) -> usize {
        self.samples.len()
    }

    /// 判断窗口是否已有足够数据（至少覆盖一半窗口时间）
    #[wasm_bindgen(js_name = "isWarmedUp")]
    pub fn is_warmed_up(&self) -> bool {
        if self.samples.len() < 2 {
            return false;
        }
        let first_ts = self.samples.front().unwrap().timestamp;
        let last_ts = self.samples.back().unwrap().timestamp;
        (last_ts - first_ts) >= self.window_ms * 0.5
    }

    /// 获取 EAR 阈值
    #[wasm_bindgen(js_name = "getThreshold")]
    pub fn get_threshold(&self) -> f64 {
        self.ear_threshold
    }

    /// 设置 EAR 阈值
    #[wasm_bindgen(js_name = "setThreshold")]
    pub fn set_threshold(&mut self, threshold: f64) {
        self.ear_threshold = threshold;
    }

    /// 重置计算器状态
    pub fn reset(&mut self) {
        self.samples.clear();
        self.current_perclos = 0.0;
    }
}

impl PERCLOSCalculator {
    /// 计算当前窗口内的 PERCLOS
    ///
    /// 使用时间加权方式：
    /// 相邻样本之间的时间段，如果前一样本为闭眼则该时间段计入闭眼时间。
    fn compute_perclos(&self) -> f64 {
        if self.samples.len() < 2 {
            return 0.0;
        }

        let mut closed_duration = 0.0;
        let mut total_duration = 0.0;

        let mut iter = self.samples.iter();
        let Some(mut prev) = iter.next() else {
            return 0.0;
        };

        for curr in iter {
            let dt = curr.timestamp - prev.timestamp;
            if dt > 0.0 {
                total_duration += dt;
                if prev.is_closed {
                    closed_duration += dt;
                }
            }
            prev = curr;
        }

        if total_duration < 1e-6 {
            return 0.0;
        }

        (closed_duration / total_duration).clamp(0.0, 1.0)
    }
}
