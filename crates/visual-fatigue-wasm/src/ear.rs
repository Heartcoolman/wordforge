//! EAR (Eye Aspect Ratio) 计算模块
//!
//! 提供基于眼部关键点的 EAR 计算，支持标准6点和增强16点两种模式。
//! EAR 值用于判断眼睛的睁闭状态，是疲劳检测的核心指标之一。

use std::collections::VecDeque;
use wasm_bindgen::prelude::*;

use crate::Point;

const EAR_16POINT_PAIRS: [(usize, usize); 7] = [
    (1, 15),
    (2, 14),
    (3, 13),
    (4, 12),
    (5, 11),
    (6, 10),
    (7, 9),
];

fn distance_from_landmarks(landmarks: &[f64], a: usize, b: usize) -> f64 {
    let ax = landmarks[a * 2];
    let ay = landmarks[a * 2 + 1];
    let bx = landmarks[b * 2];
    let by = landmarks[b * 2 + 1];

    ((ax - bx).powi(2) + (ay - by).powi(2)).sqrt()
}

/// EAR 计算结果
#[wasm_bindgen]
pub struct EARResult {
    /// EAR 值
    pub ear: f64,
    /// 置信度 (0.0 - 1.0)
    pub confidence: f64,
}

/// EAR 计算器
///
/// 支持标准6点 EAR 和增强16点 EAR 两种计算模式。
/// - 标准6点公式: EAR = (|p2-p6| + |p3-p5|) / (2 * |p1-p4|)
/// - 增强16点: 使用更多关键点提高精度，对每组上下配对取平均
#[wasm_bindgen]
pub struct EARCalculator {
    /// EAR 阈值，低于此值视为闭眼
    threshold: f64,
    /// 历史 EAR 值，用于平滑
    history: VecDeque<f64>,
    /// 平滑窗口大小
    smooth_window: usize,
}

#[wasm_bindgen]
impl EARCalculator {
    /// 创建新的 EAR 计算器
    ///
    /// # 参数
    /// - `threshold`: EAR 阈值，默认推荐 0.2
    /// - `smooth_window`: 平滑窗口大小，默认推荐 3
    #[wasm_bindgen(constructor)]
    pub fn new(threshold: f64, smooth_window: usize) -> Self {
        Self {
            threshold,
            history: VecDeque::with_capacity(100),
            smooth_window: if smooth_window == 0 { 1 } else { smooth_window },
        }
    }

    /// 标准6点 EAR 计算
    ///
    /// 输入: 12个浮点数 (6个点 × 2个坐标)，按顺序为:
    /// p1(x,y), p2(x,y), p3(x,y), p4(x,y), p5(x,y), p6(x,y)
    ///
    /// 公式: EAR = (|p2-p6| + |p3-p5|) / (2 * |p1-p4|)
    /// - p1, p4: 眼角点（水平方向）
    /// - p2, p6: 上眼睑点
    /// - p3, p5: 下眼睑点
    #[wasm_bindgen(js_name = "calculate6Point")]
    pub fn calculate_6point(&mut self, landmarks: &[f64]) -> EARResult {
        if landmarks.len() < 12 {
            return EARResult {
                ear: 0.0,
                confidence: 0.0,
            };
        }

        let p1 = Point::new(landmarks[0], landmarks[1]);
        let p2 = Point::new(landmarks[2], landmarks[3]);
        let p3 = Point::new(landmarks[4], landmarks[5]);
        let p4 = Point::new(landmarks[6], landmarks[7]);
        let p5 = Point::new(landmarks[8], landmarks[9]);
        let p6 = Point::new(landmarks[10], landmarks[11]);

        let horizontal = p1.distance(&p4);
        if horizontal < 1e-6 {
            return EARResult {
                ear: 0.0,
                confidence: 0.0,
            };
        }

        let vertical1 = p2.distance(&p6);
        let vertical2 = p3.distance(&p5);
        let ear = (vertical1 + vertical2) / (2.0 * horizontal);

        // 置信度基于水平距离的合理性（太小说明检测不可靠）
        // 输入坐标应为归一化 [0,1] 范围，阈值 0.05 基于此约定
        let confidence = (horizontal / 0.05).min(1.0);

        self.push_history(ear);

        EARResult { ear, confidence }
    }

    /// 增强16点 EAR 计算
    ///
    /// 输入: 32个浮点数 (16个点 × 2个坐标)
    /// 使用更多的上下眼睑配对点来提高精度。
    ///
    /// 点排列:
    /// - p0, p8: 左右眼角（水平方向）
    /// - p1-p7: 上眼睑点（从左到右）
    /// - p9-p15: 下眼睑点（对应配对）
    ///
    /// 取多组垂直距离的平均值作为分子。
    #[wasm_bindgen(js_name = "calculate16Point")]
    pub fn calculate_16point(&mut self, landmarks: &[f64]) -> EARResult {
        if landmarks.len() < 32 {
            return EARResult {
                ear: 0.0,
                confidence: 0.0,
            };
        }

        // p0 和 p8 是眼角点
        let horizontal = distance_from_landmarks(landmarks, 0, 8);
        if horizontal < 1e-6 {
            return EARResult {
                ear: 0.0,
                confidence: 0.0,
            };
        }

        // 计算多组上下配对的垂直距离
        // 上眼睑: p1, p2, p3, p4, p5, p6, p7
        // 下眼睑: p15, p14, p13, p12, p11, p10, p9 （对应配对）
        let vertical_sum: f64 = EAR_16POINT_PAIRS
            .iter()
            .map(|&(upper, lower)| distance_from_landmarks(landmarks, upper, lower))
            .sum();
        let vertical_avg = vertical_sum / EAR_16POINT_PAIRS.len() as f64;

        let ear = vertical_avg / horizontal;

        // 16点模式置信度更高
        // 输入坐标应为归一化 [0,1] 范围，阈值 0.03 基于此约定
        let confidence = (horizontal / 0.03).min(1.0);

        self.push_history(ear);

        EARResult { ear, confidence }
    }

    /// 获取平滑后的 EAR 值
    ///
    /// 使用最近 N 帧的移动平均来减少噪声抖动
    #[wasm_bindgen(js_name = "getSmoothedEAR")]
    pub fn get_smoothed_ear(&self) -> f64 {
        if self.history.is_empty() {
            return 0.0;
        }
        let window = self.history.len().min(self.smooth_window);
        let sum: f64 = self.history.iter().rev().take(window).sum();
        sum / window as f64
    }

    /// 判断眼睛是否闭合
    ///
    /// 基于平滑后的 EAR 值和阈值进行判断
    #[wasm_bindgen(js_name = "isEyeClosed")]
    pub fn is_eye_closed(&self) -> bool {
        self.get_smoothed_ear() < self.threshold
    }

    /// 获取当前阈值
    #[wasm_bindgen(js_name = "getThreshold")]
    pub fn get_threshold(&self) -> f64 {
        self.threshold
    }

    /// 设置阈值
    #[wasm_bindgen(js_name = "setThreshold")]
    pub fn set_threshold(&mut self, threshold: f64) {
        self.threshold = threshold;
    }

    /// 双眼 6 点联合计算：分别计算左右眼 EAR 后取平均，仅 push 一次
    ///
    /// 输入: 24 个浮点数（左眼 12 + 右眼 12）
    #[wasm_bindgen(js_name = "calculateBinocular6Point")]
    pub fn calculate_binocular_6point(&mut self, left: &[f64], right: &[f64]) -> EARResult {
        let calc = |lm: &[f64]| -> Option<(f64, f64)> {
            if lm.len() < 12 {
                return None;
            }
            let p1 = Point::new(lm[0], lm[1]);
            let p2 = Point::new(lm[2], lm[3]);
            let p3 = Point::new(lm[4], lm[5]);
            let p4 = Point::new(lm[6], lm[7]);
            let p5 = Point::new(lm[8], lm[9]);
            let p6 = Point::new(lm[10], lm[11]);
            let h = p1.distance(&p4);
            if h < 1e-6 {
                return None;
            }
            let ear = (p2.distance(&p6) + p3.distance(&p5)) / (2.0 * h);
            let conf = (h / 0.05).min(1.0);
            Some((ear, conf))
        };

        let (left_ear, left_conf) = calc(left).unwrap_or((0.0, 0.0));
        let (right_ear, right_conf) = calc(right).unwrap_or((0.0, 0.0));

        let ear = (left_ear + right_ear) / 2.0;
        let confidence = (left_conf + right_conf) / 2.0;

        self.push_history(ear);

        EARResult { ear, confidence }
    }

    /// 重置计算器状态
    pub fn reset(&mut self) {
        self.history.clear();
    }
}

impl EARCalculator {
    fn push_history(&mut self, ear: f64) {
        self.history.push_back(ear);
        while self.history.len() > 100 {
            self.history.pop_front();
        }
    }
}
