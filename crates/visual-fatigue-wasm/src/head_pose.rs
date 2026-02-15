//! 头部姿态估计模块
//!
//! 基于 MediaPipe FaceLandmarker 输出的欧拉角进行头部姿态分析。
//! 主要检测：
//! - 头部下垂（pitch 角度过大）：疲劳时典型的"点头"现象
//! - 头部倾斜（roll 角度过大）：疲劳时头部偏向一侧
//!
//! 统计下垂频率和持续时间作为疲劳指标。

use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

/// 头部下垂事件记录
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct HeadDropEvent {
    /// 下垂结束的时间戳（毫秒）
    timestamp: f64,
    /// 下垂持续时间（毫秒）
    duration: f64,
}

/// 头部姿态分析结果
#[wasm_bindgen]
pub struct HeadPoseResult {
    /// 俯仰角（度）
    pub pitch: f64,
    /// 偏航角（度）
    pub yaw: f64,
    /// 滚转角（度）
    pub roll: f64,
    /// 是否正在下垂
    pub is_dropping: bool,
    /// 是否正在倾斜
    pub is_tilting: bool,
    /// 下垂频率（次/分钟）
    pub drop_rate: f64,
    /// 窗口内下垂时间占比
    pub drop_ratio: f64,
}

/// 头部姿态估计器
///
/// 分析头部的 pitch/yaw/roll 角度，检测疲劳相关的异常姿态。
#[wasm_bindgen]
pub struct HeadPoseEstimator {
    /// 头部下垂 pitch 角度阈值（度）
    pitch_threshold: f64,
    /// 头部倾斜 roll 角度阈值（度）
    roll_threshold: f64,
    /// 是否正在下垂
    is_dropping: bool,
    /// 下垂开始时间戳
    drop_start_ts: f64,
    /// 下垂事件历史
    drop_history: VecDeque<HeadDropEvent>,
    /// 窗口大小（毫秒），默认60秒
    window_ms: f64,
    /// 用于计算下垂时间占比的样本队列
    pose_samples: VecDeque<PoseSample>,
}

/// 姿态样本
#[derive(Clone, Copy)]
struct PoseSample {
    is_dropping: bool,
    timestamp: f64,
}

#[wasm_bindgen]
impl HeadPoseEstimator {
    /// 创建新的头部姿态估计器
    ///
    /// # 参数
    /// - `pitch_threshold`: 下垂 pitch 阈值（度），推荐 15.0
    /// - `roll_threshold`: 倾斜 roll 阈值（度），推荐 20.0
    #[wasm_bindgen(constructor)]
    pub fn new(pitch_threshold: f64, roll_threshold: f64) -> Self {
        Self {
            pitch_threshold,
            roll_threshold,
            is_dropping: false,
            drop_start_ts: 0.0,
            drop_history: VecDeque::new(),
            window_ms: 60_000.0,
            pose_samples: VecDeque::new(),
        }
    }

    /// 更新头部姿态数据
    ///
    /// # 参数
    /// - `pitch`: 俯仰角（度），正值为头部向下
    /// - `yaw`: 偏航角（度），正值为头部向右
    /// - `roll`: 滚转角（度），正值为头部向右倾斜
    /// - `timestamp`: 当前时间戳（毫秒）
    pub fn update(&mut self, pitch: f64, yaw: f64, roll: f64, timestamp: f64) -> HeadPoseResult {
        let is_dropping_now = pitch > self.pitch_threshold;
        let is_tilting = roll.abs() > self.roll_threshold;

        // 检测下垂事件
        if is_dropping_now && !self.is_dropping {
            // 开始下垂
            self.is_dropping = true;
            self.drop_start_ts = timestamp;
        } else if !is_dropping_now && self.is_dropping {
            // 下垂结束
            self.is_dropping = false;
            let duration = timestamp - self.drop_start_ts;

            // 有效下垂：持续 500ms 以上
            if duration >= 500.0 {
                self.drop_history.push_back(HeadDropEvent {
                    timestamp,
                    duration,
                });
            }
        }

        // 记录姿态样本用于计算下垂时间占比
        self.pose_samples.push_back(PoseSample {
            is_dropping: is_dropping_now,
            timestamp,
        });

        // 清理窗口外的旧数据
        let cutoff = timestamp - self.window_ms;
        while let Some(front) = self.drop_history.front() {
            if front.timestamp < cutoff {
                self.drop_history.pop_front();
            } else {
                break;
            }
        }
        while let Some(front) = self.pose_samples.front() {
            if front.timestamp < cutoff {
                self.pose_samples.pop_front();
            } else {
                break;
            }
        }

        let drop_rate = self.calculate_drop_rate(timestamp);
        let drop_ratio = self.calculate_drop_ratio();

        HeadPoseResult {
            pitch,
            yaw,
            roll,
            is_dropping: is_dropping_now,
            is_tilting,
            drop_rate,
            drop_ratio,
        }
    }

    /// 获取下垂频率（次/分钟）
    #[wasm_bindgen(js_name = "getDropRate")]
    pub fn get_drop_rate(&self) -> f64 {
        if let Some(last) = self.drop_history.back() {
            self.calculate_drop_rate(last.timestamp)
        } else {
            0.0
        }
    }

    /// 获取窗口内下垂时间占比
    #[wasm_bindgen(js_name = "getDropRatio")]
    pub fn get_drop_ratio(&self) -> f64 {
        self.calculate_drop_ratio()
    }

    /// 设置 pitch 阈值
    #[wasm_bindgen(js_name = "setPitchThreshold")]
    pub fn set_pitch_threshold(&mut self, threshold: f64) {
        self.pitch_threshold = threshold;
    }

    /// 设置 roll 阈值
    #[wasm_bindgen(js_name = "setRollThreshold")]
    pub fn set_roll_threshold(&mut self, threshold: f64) {
        self.roll_threshold = threshold;
    }

    /// 重置估计器状态
    pub fn reset(&mut self) {
        self.is_dropping = false;
        self.drop_start_ts = 0.0;
        self.drop_history.clear();
        self.pose_samples.clear();
    }
}

impl HeadPoseEstimator {
    /// 计算下垂频率（次/分钟）
    fn calculate_drop_rate(&self, current_ts: f64) -> f64 {
        if self.drop_history.is_empty() {
            return 0.0;
        }

        let first_ts = self.drop_history.front().unwrap().timestamp;
        let elapsed_ms = current_ts - first_ts;

        if elapsed_ms < 10_000.0 {
            return 0.0;
        }

        let count = self.drop_history.len() as f64;
        count / (elapsed_ms / 60_000.0)
    }

    /// 计算窗口内下垂时间占比
    fn calculate_drop_ratio(&self) -> f64 {
        if self.pose_samples.len() < 2 {
            return 0.0;
        }

        let mut drop_duration = 0.0;
        let mut total_duration = 0.0;

        let mut iter = self.pose_samples.iter();
        let Some(mut prev) = iter.next() else {
            return 0.0;
        };
        for curr in iter {
            let dt = curr.timestamp - prev.timestamp;
            if dt > 0.0 {
                total_duration += dt;
                if prev.is_dropping {
                    drop_duration += dt;
                }
            }
            prev = curr;
        }

        if total_duration < 1e-6 {
            return 0.0;
        }

        (drop_duration / total_duration).clamp(0.0, 1.0)
    }
}
