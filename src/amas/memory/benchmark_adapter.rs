use serde::{Deserialize, Serialize};

use crate::amas::config::MemoryModelConfig;

use super::mdm::{compute_interval, recall_probability, update_strength, MdmState};

const DAY_MS: i64 = 86_400_000;
/// 二元 recall（1=记住）按 FSRS 二元拟合惯例映射到 Good(3)，而非 Easy(4)。
/// update_strength 的评分带为 `<=0.85 → Good(3)`、`>0.85 → Easy(4)`；
/// 取 0.7 落在 Good 带内，避免首评 stability 取 w[3]（Easy）造成约 5x 膨胀、扭曲调参与算法对比。
const SUCCESS_QUALITY: f64 = 0.7;
const FAILURE_QUALITY: f64 = 0.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkHistoryItem {
    #[serde(default)]
    pub t_history: Vec<i64>,
    #[serde(default)]
    pub r_history: Vec<u8>,
    /// Alternative: pass raw CSV strings and Rust will parse them.
    #[serde(default)]
    pub t_history_csv: Option<String>,
    #[serde(default)]
    pub r_history_csv: Option<String>,
    #[serde(default)]
    pub next_t_days: Option<f64>,
    #[serde(default)]
    pub target_retentions: Vec<f64>,
    #[serde(default = "default_interval_scale")]
    pub interval_scale: f64,
}

impl BenchmarkHistoryItem {
    /// Resolve t_history: prefer parsed array, fall back to CSV string.
    pub fn resolve_t_history(&self) -> Vec<i64> {
        if !self.t_history.is_empty() {
            return self.t_history.clone();
        }
        match &self.t_history_csv {
            Some(csv) if !csv.is_empty() => csv
                .split(',')
                .filter_map(|s| s.trim().parse::<i64>().ok())
                .collect(),
            _ => vec![],
        }
    }

    /// Resolve r_history: prefer parsed array, fall back to CSV string.
    pub fn resolve_r_history(&self) -> Vec<u8> {
        if !self.r_history.is_empty() {
            return self.r_history.clone();
        }
        match &self.r_history_csv {
            Some(csv) if !csv.is_empty() => csv
                .split(',')
                .filter_map(|s| s.trim().parse::<u8>().ok())
                .collect(),
            _ => vec![],
        }
    }
}

fn default_interval_scale() -> f64 {
    1.0
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkAdapterRequest {
    pub config: MemoryModelConfig,
    pub items: Vec<BenchmarkHistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RetentionInterval {
    pub retention: f64,
    pub interval_days: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkAdapterResult {
    pub stability: f64,
    pub difficulty: f64,
    pub review_count: u32,
    pub predicted_recall: Option<f64>,
    pub intervals: Vec<RetentionInterval>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BenchmarkAdapterResponse {
    pub items: Vec<BenchmarkAdapterResult>,
}

pub fn evaluate_batch(
    request: BenchmarkAdapterRequest,
) -> Result<BenchmarkAdapterResponse, String> {
    let mut results = Vec::with_capacity(request.items.len());

    for item in request.items {
        let state = replay_history(&item, &request.config)?;
        let now_ms = state.last_review_at.unwrap_or(0);
        let predicted_recall = item.next_t_days.map(|days| {
            recall_probability(
                &state,
                now_ms + (days * DAY_MS as f64) as i64,
                &request.config,
            )
        });
        let intervals = item
            .target_retentions
            .iter()
            .map(|&retention| {
                let seconds =
                    compute_interval(&state, retention, item.interval_scale, &request.config);
                RetentionInterval {
                    retention,
                    interval_days: seconds as f64 / 86_400.0,
                }
            })
            .collect();

        results.push(BenchmarkAdapterResult {
            stability: state.stability,
            difficulty: state.difficulty,
            review_count: state.review_count,
            predicted_recall,
            intervals,
        });
    }

    Ok(BenchmarkAdapterResponse { items: results })
}

pub fn replay_history(
    item: &BenchmarkHistoryItem,
    config: &MemoryModelConfig,
) -> Result<MdmState, String> {
    let t_history = item.resolve_t_history();
    let r_history = item.resolve_r_history();

    if t_history.len() != r_history.len() {
        return Err(format!(
            "history length mismatch: t_history={} r_history={}",
            t_history.len(),
            r_history.len()
        ));
    }

    let mut state = MdmState::default();
    let mut now_ms = 0i64;

    for (&delta_t, &result) in t_history.iter().zip(r_history.iter()) {
        if delta_t < 0 {
            return Err(format!("delta_t must be >= 0, got {delta_t}"));
        }
        now_ms += delta_t * DAY_MS;
        let quality = if result == 0 {
            FAILURE_QUALITY
        } else {
            SUCCESS_QUALITY
        };
        update_strength(&mut state, quality, 0.3, now_ms, config);
    }

    Ok(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replay_batch_matches_direct_recall_and_interval() {
        let config = MemoryModelConfig::default();
        let item = BenchmarkHistoryItem {
            t_history: vec![0, 1, 3, 7],
            r_history: vec![1, 1, 0, 1],
            t_history_csv: None,
            r_history_csv: None,
            next_t_days: Some(2.0),
            target_retentions: vec![0.8, 0.85, 0.9],
            interval_scale: 1.0,
        };

        let state = replay_history(&item, &config).expect("state");
        let now_ms = state.last_review_at.unwrap();
        let expected_recall = recall_probability(&state, now_ms + 2 * DAY_MS, &config);
        let expected_interval = compute_interval(&state, 0.85, 1.0, &config) as f64 / 86_400.0;

        let response = evaluate_batch(BenchmarkAdapterRequest {
            config,
            items: vec![item],
        })
        .expect("response");
        let scored = &response.items[0];

        assert_eq!(scored.review_count, state.review_count);
        assert!((scored.stability - state.stability).abs() < 1e-9);
        assert!((scored.predicted_recall.expect("pred") - expected_recall).abs() < 1e-9);
        let interval = scored
            .intervals
            .iter()
            .find(|entry| (entry.retention - 0.85).abs() < 1e-9)
            .expect("interval");
        assert!((interval.interval_days - expected_interval).abs() < 1e-9);
    }

    #[test]
    fn replay_history_rejects_length_mismatch() {
        let config = MemoryModelConfig::default();
        let item = BenchmarkHistoryItem {
            t_history: vec![0, 1],
            r_history: vec![1],
            t_history_csv: None,
            r_history_csv: None,
            next_t_days: None,
            target_retentions: vec![],
            interval_scale: 1.0,
        };
        let err = replay_history(&item, &config).expect_err("should fail");
        assert!(err.contains("history length mismatch"));
    }
}
