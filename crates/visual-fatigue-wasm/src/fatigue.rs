//! 综合疲劳评分模块
//!
//! 基于多维度指标进行加权综合评分：
//! - PERCLOS (闭眼时间百分比): 30%
//! - 眨眼异常: 20%
//! - 哈欠: 20%
//! - 头部下垂: 15%
//! - 表情 (blendshapes): 15%
//!
//! 疲劳等级：
//! - Alert (0-25): 清醒
//! - Mild (25-50): 轻度疲劳
//! - Moderate (50-75): 中度疲劳
//! - Severe (75-100): 严重疲劳

use std::collections::VecDeque;
use serde::Serialize;
use wasm_bindgen::prelude::*;

/// 疲劳检测综合结果
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FatigueResult {
    /// 综合疲劳评分 (0-100)
    pub score: f64,
    /// 疲劳等级: "alert" | "mild" | "moderate" | "severe"
    pub level: String,
    /// PERCLOS 百分比 (0.0-1.0)
    pub perclos: f64,
    /// 眨眼率（次/分钟）
    pub blink_rate: f64,
    /// 近期哈欠次数
    pub yawn_count: u32,
    /// 头部下垂时间占比 (0.0-1.0)
    pub head_drop_ratio: f64,
    /// 时间戳（毫秒）
    pub timestamp: f64,
}

/// 各维度权重配置
#[derive(Clone, Copy)]
struct Weights {
    perclos: f64,
    blink: f64,
    yawn: f64,
    head_drop: f64,
    expression: f64,
}

impl Default for Weights {
    fn default() -> Self {
        Self {
            perclos: 0.30,
            blink: 0.20,
            yawn: 0.20,
            head_drop: 0.15,
            expression: 0.15,
        }
    }
}

/// 综合疲劳评分器
///
/// 接收各子模块的检测结果，计算综合疲劳评分。
/// 各维度分别映射到 0-100 分后加权求和。
#[wasm_bindgen]
pub struct FatigueScorer {
    /// 权重配置
    weights: Weights,
    /// 历史评分，用于平滑输出
    score_history: VecDeque<f64>,
    /// 平滑窗口大小
    smooth_window: usize,
    /// 正常眨眼率范围
    normal_blink_min: f64,
    normal_blink_max: f64,
}

#[wasm_bindgen]
impl FatigueScorer {
    /// 创建新的疲劳评分器
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self {
            weights: Weights::default(),
            score_history: VecDeque::new(),
            smooth_window: 5,
            normal_blink_min: 15.0,
            normal_blink_max: 20.0,
        }
    }

    /// 计算综合疲劳评分
    ///
    /// # 参数
    /// - `perclos`: PERCLOS 值 (0.0-1.0)
    /// - `blink_rate`: 眨眼频率（次/分钟）
    /// - `blink_abnormal`: 眨眼是否异常
    /// - `yawn_count`: 近期哈欠次数（仅透传到 FatigueResult，不参与评分计算）
    /// - `yawn_rate`: 哈欠频率（次/分钟）
    /// - `head_drop_ratio`: 头部下垂时间占比 (0.0-1.0)
    /// - `expression_score`: 表情疲劳分数 (0.0-1.0)，从 blendshapes 获取
    /// - `timestamp`: 当前时间戳（毫秒）
    ///
    /// # 返回
    /// 序列化为 JsValue 的 FatigueResult
    #[wasm_bindgen(js_name = "calculate")]
    pub fn calculate(
        &mut self,
        perclos: f64,
        blink_rate: f64,
        blink_abnormal: bool,
        yawn_count: u32,
        yawn_rate: f64,
        head_drop_ratio: f64,
        expression_score: f64,
        timestamp: f64,
    ) -> JsValue {
        // === 各维度评分映射 (0-100) ===

        // 1. PERCLOS 评分
        // PERCLOS < 0.15 → 0分, > 0.40 → 100分
        let perclos_score = Self::map_range(perclos, 0.15, 0.40);

        // 2. 眨眼异常评分
        let blink_score = if blink_abnormal {
            if blink_rate < self.normal_blink_min {
                // 眨眼率过低（注意力涣散或困倦）
                Self::map_range(
                    self.normal_blink_min - blink_rate,
                    0.0,
                    self.normal_blink_min,
                )
            } else {
                // 眨眼率过高（眼睛干涩疲劳）
                Self::map_range(
                    blink_rate - self.normal_blink_max,
                    0.0,
                    self.normal_blink_max,
                )
            }
        } else {
            0.0 // 正常眨眼率 → 0分
        };

        // 3. 哈欠评分
        // 哈欠率 > 0 就开始计分，> 3次/5分钟 → 高分
        let yawn_score = Self::map_range(yawn_rate, 0.0, 1.0);

        // 4. 头部下垂评分
        // 下垂占比 > 5% 开始计分，> 30% → 100分
        let head_score = Self::map_range(head_drop_ratio, 0.05, 0.30);

        // 5. 表情评分（直接使用归一化后的值）
        let expr_score = (expression_score * 100.0).clamp(0.0, 100.0);

        // === 加权综合 ===
        let raw_score = self.weights.perclos * perclos_score
            + self.weights.blink * blink_score
            + self.weights.yawn * yawn_score
            + self.weights.head_drop * head_score
            + self.weights.expression * expr_score;

        let score = raw_score.clamp(0.0, 100.0);

        // 平滑处理
        self.score_history.push_back(score);
        while self.score_history.len() > 100 {
            self.score_history.pop_front();
        }

        let smoothed_score = self.get_smoothed_score();
        let level = Self::score_to_level(smoothed_score);

        let result = FatigueResult {
            score: smoothed_score,
            level,
            perclos,
            blink_rate,
            yawn_count,
            head_drop_ratio,
            timestamp,
        };

        serde_wasm_bindgen::to_value(&result).unwrap_or(JsValue::NULL)
    }

    /// 获取平滑后的评分
    #[wasm_bindgen(js_name = "getSmoothedScore")]
    pub fn get_smoothed_score(&self) -> f64 {
        if self.score_history.is_empty() {
            return 0.0;
        }
        let window = self.score_history.len().min(self.smooth_window);
        let sum: f64 = self.score_history.iter().rev().take(window).sum();
        (sum / window as f64).clamp(0.0, 100.0)
    }

    /// 获取当前疲劳等级
    #[wasm_bindgen(js_name = "getLevel")]
    pub fn get_level(&self) -> String {
        Self::score_to_level(self.get_smoothed_score())
    }

    /// 设置各维度权重
    ///
    /// 权重会自动归一化，确保总和为1。
    #[wasm_bindgen(js_name = "setWeights")]
    pub fn set_weights(
        &mut self,
        perclos: f64,
        blink: f64,
        yawn: f64,
        head_drop: f64,
        expression: f64,
    ) {
        let total = perclos + blink + yawn + head_drop + expression;
        if total > 1e-6 {
            self.weights = Weights {
                perclos: perclos / total,
                blink: blink / total,
                yawn: yawn / total,
                head_drop: head_drop / total,
                expression: expression / total,
            };
        }
    }

    /// 设置平滑窗口大小
    #[wasm_bindgen(js_name = "setSmoothWindow")]
    pub fn set_smooth_window(&mut self, window: usize) {
        self.smooth_window = if window == 0 { 1 } else { window };
    }

    /// 重置评分器状态
    pub fn reset(&mut self) {
        self.score_history.clear();
    }
}

impl FatigueScorer {
    /// 线性映射：将值从 [low, high] 映射到 [0, 100]
    fn map_range(value: f64, low: f64, high: f64) -> f64 {
        if high <= low {
            return 0.0;
        }
        ((value - low) / (high - low) * 100.0).clamp(0.0, 100.0)
    }

    /// 分数转疲劳等级
    fn score_to_level(score: f64) -> String {
        match score {
            s if s < 25.0 => "alert".to_string(),
            s if s < 50.0 => "mild".to_string(),
            s if s < 75.0 => "moderate".to_string(),
            _ => "severe".to_string(),
        }
    }
}
