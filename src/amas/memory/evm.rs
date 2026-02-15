//! B39: Encoding Variability Model (EVM)
//! Context diversity metric modifies interval scaling.

use serde::{Deserialize, Serialize};

const DIVERSITY_LOG_DIVISOR: f64 = 5.0;
const DIVERSITY_BONUS_CAP: f64 = 0.3;
const DIVERSITY_GROWTH_RATE: f64 = 0.2;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EvmState {
    /// Number of distinct contexts a word has been studied in
    pub context_count: u32,
    /// Context diversity score (0-1)
    pub diversity_score: f64,
}

/// Calculate the encoding variability bonus.
/// More diverse contexts -> better encoding -> longer intervals.
pub fn context_diversity_bonus(state: &EvmState) -> f64 {
    // Logarithmic scaling: diminishing returns after many contexts
    let diversity = (1.0 + state.context_count as f64).ln() / DIVERSITY_LOG_DIVISOR.ln();
    (diversity * state.diversity_score).clamp(0.0, DIVERSITY_BONUS_CAP)
}

/// Update EVM state when a word is studied in a new context
pub fn record_context(state: &mut EvmState, is_new_context: bool) {
    if is_new_context {
        state.context_count += 1;
    }
    // Update diversity score based on context count
    state.diversity_score = (1.0 - (-DIVERSITY_GROWTH_RATE * state.context_count as f64).exp()).clamp(0.0, 1.0);
}

/// Modify interval scaling based on encoding variability
pub fn interval_modifier(state: &EvmState) -> f64 {
    1.0 + context_diversity_bonus(state)
}
