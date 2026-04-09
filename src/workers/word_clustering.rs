//! B73: Word clustering (weekly Sunday 4:00)

use crate::store::Store;

const DIFFICULTY_EASY_THRESHOLD: f64 = 0.33;
const DIFFICULTY_MEDIUM_THRESHOLD: f64 = 0.66;

pub async fn run(store: &Store) {
    tracing::info!("Word clustering worker running");

    let words = match store.list_all_words_with_tags() {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to load words for clustering");
            return;
        }
    };

    let mut easy = 0u32;
    let mut medium = 0u32;
    let mut hard = 0u32;
    let total_count = words.len();
    let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    for (_id, difficulty, tags) in &words {
        if *difficulty < DIFFICULTY_EASY_THRESHOLD {
            easy += 1;
        } else if *difficulty < DIFFICULTY_MEDIUM_THRESHOLD {
            medium += 1;
        } else {
            hard += 1;
        }

        for tag in tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
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
