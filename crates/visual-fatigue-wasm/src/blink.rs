//! 眨眼检测模块
//!
//! 基于四状态有限状态机进行眨眼检测：
//! Open（睁眼）→ Closing（正在闭眼）→ Closed（闭眼）→ Opening（正在睁眼）→ Open
//!
//! 通过 EAR 值的变化趋势来判断状态转换，并统计眨眼频率。
//! 正常眨眼频率为 15-20 次/分钟，过低或过高都可能是疲劳的信号。

use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

/// 眼睛状态
#[derive(Clone, Copy, PartialEq, Debug)]
enum EyeState {
    /// 睁眼状态
    Open,
    /// 正在闭眼（EAR 下降中）
    Closing,
    /// 闭眼状态
    Closed,
    /// 正在睁眼（EAR 上升中）
    Opening,
}

/// 眨眼事件记录
#[derive(Clone, Copy)]
#[allow(dead_code)]
struct BlinkEvent {
    /// 眨眼完成的时间戳（毫秒）
    timestamp: f64,
    /// 眨眼持续时间（毫秒）
    duration: f64,
}

/// 眨眼检测结果
#[wasm_bindgen]
pub struct BlinkResult {
    /// 是否刚完成一次眨眼
    pub blink_detected: bool,
    /// 当前眨眼频率（次/分钟）
    pub blink_rate: f64,
    /// 眨眼频率是否异常
    pub is_abnormal: bool,
    /// 当前状态（0=Open, 1=Closing, 2=Closed, 3=Opening）
    pub state: u8,
}

/// 眨眼检测器
///
/// 基于四状态 FSM 检测眨眼事件，并统计频率。
/// 使用60秒的滑动窗口计算眨眼频率。
#[wasm_bindgen]
pub struct BlinkDetector {
    /// EAR 闭眼阈值
    close_threshold: f64,
    /// EAR 睁眼阈值（略高于闭眼阈值，形成迟滞区间）
    open_threshold: f64,
    /// 当前眼睛状态
    state: EyeState,
    /// 开始闭眼的时间戳
    close_start_ts: f64,
    /// 眨眼事件历史（用于计算频率）
    blink_history: VecDeque<BlinkEvent>,
    /// 频率计算窗口大小（毫秒）
    window_ms: f64,
    /// 正常眨眼率范围下限（次/分钟）
    normal_rate_min: f64,
    /// 正常眨眼率范围上限（次/分钟）
    normal_rate_max: f64,
}

#[wasm_bindgen]
impl BlinkDetector {
    /// 创建新的眨眼检测器
    ///
    /// # 参数
    /// - `close_threshold`: 闭眼 EAR 阈值，推荐 0.2
    /// - `open_threshold`: 睁眼 EAR 阈值，推荐 0.25（形成迟滞区间防抖动）
    #[wasm_bindgen(constructor)]
    pub fn new(close_threshold: f64, open_threshold: f64) -> Self {
        Self {
            close_threshold,
            open_threshold,
            state: EyeState::Open,
            close_start_ts: 0.0,
            blink_history: VecDeque::new(),
            window_ms: 60_000.0, // 60秒窗口
            normal_rate_min: 15.0,
            normal_rate_max: 20.0,
        }
    }

    /// 更新检测器，输入当前帧的 EAR 值和时间戳
    ///
    /// # 参数
    /// - `ear`: 当前帧的 EAR 值
    /// - `timestamp`: 当前时间戳（毫秒）
    ///
    /// # 返回
    /// 眨眼检测结果
    pub fn update(&mut self, ear: f64, timestamp: f64) -> BlinkResult {
        let mut blink_detected = false;

        // 状态机转换逻辑
        match self.state {
            EyeState::Open => {
                if ear < self.close_threshold {
                    // 睁眼 → 正在闭眼
                    self.state = EyeState::Closing;
                    self.close_start_ts = timestamp;
                }
            }
            EyeState::Closing => {
                if ear < self.close_threshold {
                    // 确认进入闭眼状态（EAR 持续低于阈值）
                    self.state = EyeState::Closed;
                } else if ear >= self.open_threshold {
                    // 快速反弹回睁眼，可能是噪声，直接回到 Open
                    self.state = EyeState::Open;
                }
            }
            EyeState::Closed => {
                if ear >= self.close_threshold {
                    // 闭眼 → 正在睁眼
                    self.state = EyeState::Opening;
                }
            }
            EyeState::Opening => {
                if ear >= self.open_threshold {
                    // 完成一次眨眼
                    self.state = EyeState::Open;
                    let duration = timestamp - self.close_start_ts;

                    // 有效眨眼持续时间：50ms - 500ms
                    // 太短可能是噪声，太长可能是闭眼休息
                    if duration >= 50.0 && duration <= 500.0 {
                        blink_detected = true;
                        self.blink_history.push_back(BlinkEvent {
                            timestamp,
                            duration,
                        });
                    }
                } else if ear < self.close_threshold {
                    // 又闭回去了
                    self.state = EyeState::Closed;
                }
            }
        }

        // 清理窗口外的旧事件
        let cutoff = timestamp - self.window_ms;
        while let Some(front) = self.blink_history.front() {
            if front.timestamp < cutoff {
                self.blink_history.pop_front();
            } else {
                break;
            }
        }

        let blink_rate = self.calculate_blink_rate(timestamp);
        let is_abnormal = blink_rate > 0.0
            && (blink_rate < self.normal_rate_min || blink_rate > self.normal_rate_max);

        BlinkResult {
            blink_detected,
            blink_rate,
            is_abnormal,
            state: self.state_to_u8(),
        }
    }

    /// 获取当前眨眼频率（次/分钟）
    #[wasm_bindgen(js_name = "getBlinkRate")]
    pub fn get_blink_rate(&self) -> f64 {
        if let Some(last) = self.blink_history.back() {
            self.calculate_blink_rate(last.timestamp)
        } else {
            0.0
        }
    }

    /// 获取窗口内的眨眼次数
    #[wasm_bindgen(js_name = "getBlinkCount")]
    pub fn get_blink_count(&self) -> usize {
        self.blink_history.len()
    }

    /// 设置正常眨眼率范围
    #[wasm_bindgen(js_name = "setNormalRange")]
    pub fn set_normal_range(&mut self, min: f64, max: f64) {
        self.normal_rate_min = min;
        self.normal_rate_max = max;
    }

    /// 重置检测器状态
    pub fn reset(&mut self) {
        self.state = EyeState::Open;
        self.close_start_ts = 0.0;
        self.blink_history.clear();
    }
}

impl BlinkDetector {
    /// 计算当前眨眼频率（次/分钟）
    fn calculate_blink_rate(&self, current_ts: f64) -> f64 {
        if self.blink_history.is_empty() {
            return 0.0;
        }

        // 计算实际窗口覆盖时间（可能不足60秒）
        let first_ts = self.blink_history.front().unwrap().timestamp;
        let elapsed_ms = current_ts - first_ts;

        if elapsed_ms < 1000.0 {
            // 数据不足1秒，不计算频率
            return 0.0;
        }

        let count = self.blink_history.len() as f64;
        // 换算为每分钟的频率
        count / (elapsed_ms / 60_000.0)
    }

    /// 将状态转为数字编码
    fn state_to_u8(&self) -> u8 {
        match self.state {
            EyeState::Open => 0,
            EyeState::Closing => 1,
            EyeState::Closed => 2,
            EyeState::Opening => 3,
        }
    }
}
