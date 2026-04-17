//! 实验性功能：词源自动生成
//! 当前使用占位符文本，需要外部 LLM API 才能生成真实词源。
//! 在 workers/mod.rs 的 planned_jobs() 中默认禁用（enabled: false），
//! 启用前请确保已配置并测试 LLM provider。

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Etymology generation worker running");
    let store = store.clone();
    match crate::blocking::run_blocking("worker.etymology_generation", move || {
        let words_to_process = match store.list_words_without_etymology(50) {
            Ok(w) => w,
            Err(e) => {
                tracing::warn!(error = %e, "Failed to list words without etymology");
                return;
            }
        };

        for word_val in &words_to_process {
            let word_id = word_val["id"].as_str().unwrap_or_default();
            let word_text = word_val["text"].as_str().unwrap_or_default();

            let etymology = serde_json::json!({
                "word_id": word_id,
                "word": word_text,
                "etymology": format!("Auto-generated etymology for '{}'", word_text),
                "roots": [],
                "generated": true,
                "generated_at": chrono::Utc::now().to_rfc3339(),
            });

            if let Err(e) = store.set_etymology(word_id, &etymology) {
                tracing::warn!(word_id, error = %e, "Failed to store etymology");
            }
        }

        tracing::info!(
            processed = words_to_process.len(),
            "Etymology generation complete"
        );
    })
    .await
    {
        Ok(()) => {}
        Err(e) => tracing::warn!(error = %e, "Etymology generation task failed"),
    }
}
