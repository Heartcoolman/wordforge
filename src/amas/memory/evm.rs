//! B39: Encoding Variability Model (EVM)
//! Context diversity metric modifies interval scaling.

use serde::{Deserialize, Serialize};

use crate::amas::config::EvmConfig;

/// 记录"近期已见上下文(session)"以判定 distinct——上限防无界增长。
const SEEN_CONTEXTS_CAP: usize = 64;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvmState {
    /// Number of distinct contexts a word has been studied in
    pub context_count: u32,
    /// Context diversity score (0-1)
    pub diversity_score: f64,
    /// #46:已记入 context_count 的近期 session id 集合。仅"未见过的新 session"才使
    /// context_count +1，避免把"同一 session 的多次复习"或"无 session 的事件"误计为新上下文。
    /// 超过 [`SEEN_CONTEXTS_CAP`] 时丢弃最旧项(保留上限防状态无界膨胀)。
    #[serde(default)]
    pub seen_context_ids: Vec<String>,
}

/// Calculate the encoding variability bonus.
/// More diverse contexts -> better encoding -> longer intervals.
pub fn context_diversity_bonus(state: &EvmState, cfg: &EvmConfig) -> f64 {
    let diversity = (1.0 + state.context_count as f64).ln() / cfg.diversity_log_divisor.ln();
    (diversity * state.diversity_score).clamp(0.0, cfg.diversity_bonus_cap)
}

/// Update EVM state when a word is studied in a new context.
///
/// #46:`session_id` 是本次学习所处上下文。仅当它非空且此前未在 `seen_context_ids` 记过时，
/// 才把 `context_count` +1（真正的 distinct 上下文计数），而非"每条带 session 的复习都 +1"。
/// 无 session（`None`）的事件不计上下文。
pub fn record_context(state: &mut EvmState, session_id: Option<&str>, cfg: &EvmConfig) {
    if let Some(sid) = session_id.filter(|s| !s.is_empty()) {
        if !state.seen_context_ids.iter().any(|s| s == sid) {
            state.context_count += 1;
            state.seen_context_ids.push(sid.to_string());
            if state.seen_context_ids.len() > SEEN_CONTEXTS_CAP {
                state.seen_context_ids.remove(0);
            }
        }
    }
    state.diversity_score =
        (1.0 - (-cfg.diversity_growth_rate * state.context_count as f64).exp()).clamp(0.0, 1.0);
}

/// Modify interval scaling based on encoding variability
pub fn interval_modifier(state: &EvmState, cfg: &EvmConfig) -> f64 {
    1.0 + context_diversity_bonus(state, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> EvmConfig {
        EvmConfig::default()
    }

    #[test]
    fn same_session_does_not_inflate_context_count() {
        // #46：同一 session 反复复习同一词，context_count 应停在 1，而非每次 +1。
        let mut st = EvmState::default();
        for _ in 0..5 {
            record_context(&mut st, Some("session-A"), &cfg());
        }
        assert_eq!(st.context_count, 1);
    }

    #[test]
    fn distinct_sessions_increment_context_count() {
        let mut st = EvmState::default();
        record_context(&mut st, Some("session-A"), &cfg());
        record_context(&mut st, Some("session-B"), &cfg());
        record_context(&mut st, Some("session-A"), &cfg()); // 重复 A 不再计
        record_context(&mut st, Some("session-C"), &cfg());
        assert_eq!(st.context_count, 3);
    }

    #[test]
    fn missing_session_does_not_count_as_context() {
        let mut st = EvmState::default();
        record_context(&mut st, None, &cfg());
        record_context(&mut st, Some(""), &cfg());
        assert_eq!(st.context_count, 0);
    }

    #[test]
    fn seen_context_ids_is_capped() {
        let mut st = EvmState::default();
        for i in 0..(SEEN_CONTEXTS_CAP + 20) {
            record_context(&mut st, Some(&format!("s{i}")), &cfg());
        }
        assert!(st.seen_context_ids.len() <= SEEN_CONTEXTS_CAP);
    }
}
