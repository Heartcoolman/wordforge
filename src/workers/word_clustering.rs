//! B73: Word clustering (weekly Sunday 4:00)

use crate::store::Store;

/// 每批加载的单词数量
const WORD_PAGE_SIZE: usize = 5000;

/// 难度分级阈值：低于此值为 easy
const DIFFICULTY_EASY_THRESHOLD: f64 = 0.33;
/// 难度分级阈值：低于此值为 medium，其余为 hard
const DIFFICULTY_MEDIUM_THRESHOLD: f64 = 0.66;

pub async fn run(store: &Store) {
    tracing::info!("Word clustering worker running");

    // 分页加载单词，避免一次性加载全量到内存
    let mut easy = 0u32;
    let mut medium = 0u32;
    let mut hard = 0u32;
    let mut total_count = 0usize;
    let mut tag_counts: std::collections::HashMap<String, u32> = std::collections::HashMap::new();

    let mut offset = 0usize;
    loop {
        let words = match store.list_words(WORD_PAGE_SIZE, offset) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list words for clustering");
                return;
            }
        };

        if words.is_empty() {
            break;
        }

        let batch_len = words.len();
        total_count += batch_len;

        for word in &words {
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
        }

        offset += batch_len;

        if batch_len < WORD_PAGE_SIZE {
            break;
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
