//! 眨眼检测模块
//!
//! 基于四状态有限状态机进行眨眼检测：
//! Open（睁眼）→ Closing（正在闭眼）→ Closed（闭眼）→ Opening（正在睁眼）→ Open
//!
//! 通过 EAR 值的变化趋势来判断状态转换，并统计眨眼频率。
//! 正常眨眼频率为 15-20 次/分钟，过低或过高都可能是疲劳的信号。

use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

const MIN_BLINK_DURATION_MS: f64 = 80.0;

#[derive(Clone, Copy, PartialEq, Debug)]
enum EyeState {
    Open,
    Closing { start_ts: f64 },
    Closed,
    Opening,
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
struct BlinkEvent {
    timestamp: f64,
    duration: f64,
}

/// 眨眼检测结果
#[wasm_bindgen]
pub struct BlinkResult {
    pub blink_detected: bool,
    pub blink_rate: f64,
    pub is_abnormal: bool,
    /// 当前状态（0=Open, 1=Closing, 2=Closed, 3=Opening）
    pub state: u8,
}

/// 眨眼检测器
#[wasm_bindgen]
pub struct BlinkDetector {
    close_threshold: f64,
    open_threshold: f64,
    state: EyeState,
    close_start_ts: f64,
    blink_history: VecDeque<BlinkEvent>,
    window_ms: f64,
    normal_rate_min: f64,
    normal_rate_max: f64,
}

#[wasm_bindgen]
impl BlinkDetector {
    #[wasm_bindgen(constructor)]
    pub fn new(close_threshold: f64, open_threshold: f64) -> Self {
        Self {
            close_threshold,
            open_threshold,
            state: EyeState::Open,
            close_start_ts: 0.0,
            blink_history: VecDeque::new(),
            window_ms: 60_000.0,
            normal_rate_min: 15.0,
            normal_rate_max: 20.0,
        }
    }

    pub fn update(&mut self, ear: f64, timestamp: f64) -> BlinkResult {
        let mut blink_detected = false;

        match self.state {
            EyeState::Open => {
                if ear < self.close_threshold {
                    self.state = EyeState::Closing { start_ts: timestamp };
                    self.close_start_ts = timestamp;
                }
            }
            EyeState::Closing { start_ts } => {
                if ear < self.close_threshold {
                    if timestamp - start_ts >= MIN_BLINK_DURATION_MS {
                        self.state = EyeState::Closed;
                    }
                } else if ear >= self.open_threshold {
                    self.state = EyeState::Open;
                }
            }
            EyeState::Closed => {
                if ear >= self.close_threshold {
                    self.state = EyeState::Opening;
                }
            }
            EyeState::Opening => {
                if ear >= self.open_threshold {
                    self.state = EyeState::Open;
                    let duration = timestamp - self.close_start_ts;

                    if duration >= 50.0 && duration <= 500.0 {
                        blink_detected = true;
                        self.blink_history.push_back(BlinkEvent {
                            timestamp,
                            duration,
                        });
                    }
                } else if ear < self.close_threshold {
                    self.state = EyeState::Closed;
                }
            }
        }

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

    #[wasm_bindgen(js_name = "getBlinkRate")]
    pub fn get_blink_rate(&self) -> f64 {
        if let Some(last) = self.blink_history.back() {
            self.calculate_blink_rate(last.timestamp)
        } else {
            0.0
        }
    }

    #[wasm_bindgen(js_name = "getBlinkCount")]
    pub fn get_blink_count(&self) -> usize {
        self.blink_history.len()
    }

    #[wasm_bindgen(js_name = "setNormalRange")]
    pub fn set_normal_range(&mut self, min: f64, max: f64) {
        self.normal_rate_min = min;
        self.normal_rate_max = max;
    }

    pub fn reset(&mut self) {
        self.state = EyeState::Open;
        self.close_start_ts = 0.0;
        self.blink_history.clear();
    }
}

impl BlinkDetector {
    fn calculate_blink_rate(&self, current_ts: f64) -> f64 {
        if self.blink_history.is_empty() {
            return 0.0;
        }

        let first_ts = self.blink_history.front().unwrap().timestamp;
        let elapsed_ms = current_ts - first_ts;

        if elapsed_ms < 10_000.0 {
            return 0.0;
        }

        let count = self.blink_history.len() as f64;
        count / (elapsed_ms / 60_000.0)
    }

    fn state_to_u8(&self) -> u8 {
        match self.state {
            EyeState::Open => 0,
            EyeState::Closing { .. } => 1,
            EyeState::Closed => 2,
            EyeState::Opening => 3,
        }
    }
}
