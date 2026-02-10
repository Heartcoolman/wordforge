//! B73: Word clustering (weekly Sunday 4:00)

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Word clustering worker running");

    // Collect all words and attempt basic clustering
    let words = match store.list_words(usize::MAX, 0) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to list words for clustering");
            return;
        }
    };

    // Group by difficulty buckets
    let mut easy = 0u32;
    let mut medium = 0u32;
    let mut hard = 0u32;

    for word in &words {
        if word.difficulty < 0.33 {
            easy += 1;
        } else if word.difficulty < 0.66 {
            medium += 1;
        } else {
            hard += 1;
        }
    }

    // Group by tags
    let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();
    for word in &words {
        for tag in &word.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
        }
    }

    let date = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let report = serde_json::json!({
        "date": date,
        "totalWords": words.len(),
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
        total = words.len(),
        easy, medium, hard,
        "Word clustering complete"
    );
}
