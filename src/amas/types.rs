use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

const DEFAULT_ATTENTION: f64 = 0.7;
const DEFAULT_CONFIDENCE: f64 = 0.1;
const DEFAULT_ACTIVE_HOURS: [u8; 3] = [9, 14, 20];
const DEFAULT_SESSION_MINUTES: f64 = 15.0;
const DEFAULT_BREAKS_PER_SESSION: f64 = 1.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RawEvent {
    pub word_id: String,
    pub is_correct: bool,
    pub response_time_ms: i64,
    pub session_id: Option<String>,
    pub is_quit: bool,
    pub dwell_time_ms: Option<i64>,
    pub pause_count: Option<i32>,
    pub switch_count: Option<i32>,
    pub retry_count: Option<i32>,
    pub focus_loss_duration_ms: Option<i64>,
    pub interaction_density: Option<f64>,
    pub paused_time_ms: Option<i64>,
    pub hint_used: bool,
    #[serde(default)]
    pub confused_with: Option<String>,
}

impl Default for RawEvent {
    fn default() -> Self {
        Self {
            word_id: "".to_string(),
            is_correct: false,
            response_time_ms: 1000,
            session_id: None,
            is_quit: false,
            dwell_time_ms: None,
            pause_count: None,
            switch_count: None,
            retry_count: None,
            focus_loss_duration_ms: None,
            interaction_density: None,
            paused_time_ms: None,
            hint_used: false,
            confused_with: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessOptions {
    pub skip_monitoring: bool,
    pub force_heuristic: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeatureVector {
    pub accuracy: f64,
    pub response_speed: f64,
    pub quality: f64,
    pub engagement: f64,
    pub hint_penalty: f64,
    pub time_since_last_event_secs: f64,
    pub session_event_count: u32,
    pub is_quit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserState {
    pub attention: f64,
    pub fatigue: f64,
    pub motivation: f64,
    pub confidence: f64,
    pub last_active_at: Option<DateTime<Utc>>,
    pub session_event_count: u32,
    pub total_event_count: u64,
    pub created_at: DateTime<Utc>,
    // B25: Extended modeling fields
    #[serde(default)]
    pub cognitive_profile: CognitiveProfile,
    #[serde(default)]
    pub trend_state: TrendState,
    #[serde(default)]
    pub habit_profile: HabitProfile,
    #[serde(default)]
    pub last_session_id: Option<String>,
}

impl Default for UserState {
    fn default() -> Self {
        Self {
            attention: DEFAULT_ATTENTION,
            fatigue: 0.0,
            motivation: 0.0,
            confidence: DEFAULT_CONFIDENCE,
            last_active_at: None,
            session_event_count: 0,
            total_event_count: 0,
            created_at: Utc::now(),
            cognitive_profile: CognitiveProfile::default(),
            trend_state: TrendState::default(),
            habit_profile: HabitProfile::default(),
            last_session_id: None,
        }
    }
}

// B25: Cognitive profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CognitiveProfile {
    pub memory_capacity: f64,
    pub processing_speed: f64,
    pub stability: f64,
}

impl Default for CognitiveProfile {
    fn default() -> Self {
        Self {
            memory_capacity: 0.5,
            processing_speed: 0.5,
            stability: 0.5,
        }
    }
}

// B25: Trend state
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TrendState {
    pub accuracy_trend: f64,
    pub speed_trend: f64,
    pub engagement_trend: f64,
}

impl Default for TrendState {
    fn default() -> Self {
        Self {
            accuracy_trend: 0.0,
            speed_trend: 0.0,
            engagement_trend: 0.0,
        }
    }
}

// B25: Habit profile
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HabitProfile {
    pub preferred_hours: Vec<u8>,
    pub median_session_length_mins: f64,
    pub sessions_per_day: f64,
    #[serde(default)]
    pub temporal_performance: TemporalPerformance,
}

impl Default for HabitProfile {
    fn default() -> Self {
        Self {
            preferred_hours: DEFAULT_ACTIVE_HOURS.to_vec(),
            median_session_length_mins: DEFAULT_SESSION_MINUTES,
            sessions_per_day: DEFAULT_BREAKS_PER_SESSION,
            temporal_performance: TemporalPerformance::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TemporalPerformance {
    pub hourly_stats: Vec<HourlyStats>,
    pub total_sessions: u32,
}

impl Default for TemporalPerformance {
    fn default() -> Self {
        Self {
            hourly_stats: (0..24).map(|_| HourlyStats::default()).collect(),
            total_sessions: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HourlyStats {
    pub session_count: u32,
    pub avg_accuracy: f64,
    pub avg_response_time_ms: f64,
    pub mastery_efficiency: f64,
}

impl Default for HourlyStats {
    fn default() -> Self {
        Self {
            session_count: 0,
            avg_accuracy: 0.0,
            avg_response_time_ms: 0.0,
            mastery_efficiency: 0.0,
        }
    }
}

// B28: Learner types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LearnerType {
    Fast,
    Stable,
    Cautious,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StrategyParams {
    pub difficulty: f64,
    pub batch_size: u32,
    pub new_ratio: f64,
    pub interval_scale: f64,
    pub review_mode: bool,
}

impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            difficulty: 0.5,
            batch_size: 10,
            new_ratio: 0.3,
            interval_scale: 1.0,
            review_mode: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Reward {
    pub value: f64,
    pub components: RewardComponents,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RewardComponents {
    pub accuracy_reward: f64,
    pub speed_reward: f64,
    pub fatigue_penalty: f64,
    pub frustration_penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ObjectiveEvaluation {
    pub score: f64,
    pub retention_gain: f64,
    pub accuracy_gain: f64,
    pub speed_gain: f64,
    pub fatigue_penalty: f64,
    pub frustration_penalty: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Explanation {
    pub primary_reason: String,
    pub factors: Vec<ExplanationFactor>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplanationFactor {
    pub name: String,
    pub value: f64,
    pub impact: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordMasteryDecision {
    pub word_id: String,
    pub memory_strength: f64,
    pub recall_probability: f64,
    pub next_review_interval_secs: i64,
    pub mastery_level: MasteryLevel,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MasteryLevel {
    New,
    Learning,
    Reviewing,
    Mastered,
    Forgotten,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProcessResult {
    pub session_id: String,
    pub strategy: StrategyParams,
    pub explanation: Explanation,
    pub state: UserState,
    pub word_mastery: Option<WordMasteryDecision>,
    pub reward: Reward,
    pub cold_start_phase: Option<ColdStartPhase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ColdStartPhase {
    Classify,
    Explore,
    /// 预留：冷启动结束后进入利用阶段，当前由 None 表示
    #[allow(dead_code)]
    Exploit,
}

#[derive(Debug, Clone)]
pub struct DecisionCandidate {
    pub algorithm_id: AlgorithmId,
    pub strategy: StrategyParams,
    pub confidence: f64,
    pub explanation: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum AlgorithmId {
    Heuristic,
    Ige,
    Swd,
    Ensemble,
    Mdm,
    Mastery,
}

impl AlgorithmId {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Heuristic => "heuristic",
            Self::Ige => "ige",
            Self::Swd => "swd",
            Self::Ensemble => "ensemble",
            Self::Mdm => "mdm",
            Self::Mastery => "mastery",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_in_safe_ranges() {
        let strategy = StrategyParams::default();
        assert!((0.0..=1.0).contains(&strategy.difficulty));
        assert!((0.0..=1.0).contains(&strategy.new_ratio));
        assert!(strategy.batch_size >= 1);
    }

    #[test]
    fn serde_roundtrip() {
        let state = UserState::default();
        let encoded = serde_json::to_string(&state).unwrap();
        let decoded: UserState = serde_json::from_str(&encoded).unwrap();
        assert_eq!(decoded.session_event_count, state.session_event_count);
    }
}
