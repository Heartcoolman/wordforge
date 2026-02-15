//! B73: Word clustering (weekly Sunday 4:00)

use crate::store::Store;

const WORD_PAGE_SIZE: usize = 5000;
const DIFFICULTY_EASY_THRESHOLD: f64 = 0.33;
const DIFFICULTY_MEDIUM_THRESHOLD: f64 = 0.66;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct WordMinimal {
    difficulty: f64,
    tags: Vec<String>,
}

pub async fn run(store: &Store) {
    tracing::info!("Word clustering worker running");

    let mut easy = 0u32;
    let mut medium = 0u32;
    let mut hard = 0u32;
    let mut total_count = 0usize;
    let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    let mut scanned = 0usize;
    for item in store.words.iter() {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to read word for clustering");
                continue;
            }
        };

        let word: WordMinimal = match serde_json::from_slice(&v) {
            Ok(w) => w,
            Err(_) => continue,
        };

        total_count += 1;

        if word.difficulty < DIFFICULTY_EASY_THRESHOLD {
            easy += 1;
        } else if word.difficulty < DIFFICULTY_MEDIUM_THRESHOLD {
            medium += 1;
        } else {
            hard += 1;
        }

        for tag in &word.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }

        scanned += 1;
        if scanned % WORD_PAGE_SIZE == 0 {
            tokio::task::yield_now().await;
        }
    }

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let report = serde_json::json!({
        "date": date,
        "totalWords": total_count,
        "difficultyDistribution": {
            "easy": easy,
            "medium": medium,
            "hard": hard,
        },
        "topTags": tag_counts,
    });

    if let Err(e) = store.upsert_metrics_daily(&date, "word_clustering", &report) {
        tracing::warn!(error = %e, "Failed to store clustering report");
    }

    tracing::info!(
        total = total_count,
        easy,
        medium,
        hard,
        "Word clustering complete"
    );
}
