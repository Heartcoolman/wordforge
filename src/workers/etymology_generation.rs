//! 实验性功能：词源自动生成
//! 当前使用占位符文本，需要外部 LLM API 才能生成真实词源。
//! 在 workers/mod.rs 的 planned_jobs() 中默认禁用（enabled: false），
//! 启用前请确保已配置并测试 LLM provider。

use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::info!("Etymology generation worker running");

    let mut words_to_process = Vec::new();

    for item in store.words.iter() {
        let (_, v) = match item {
            Ok(kv) => kv,
            Err(_) => continue,
        };

        let word: crate::store::operations::words::Word = match serde_json::from_slice(&v) {
            Ok(w) => w,
            Err(_) => continue,
        };

        let key = match crate::store::keys::etymology_key(&word.id) {
            Ok(k) => k,
            Err(_) => continue,
        };
        if store
            .etymologies
            .get(key.as_bytes())
            .ok()
            .flatten()
            .is_none()
        {
            words_to_process.push(word);
        }

        if words_to_process.len() >= 50 {
            break;
        }
    }

    for word in &words_to_process {
        let etymology = serde_json::json!({
            "wordId": word.id,
            "word": word.text,
            // TODO: 接入 LLM API 生成真实词源，当前为占位文本
            "etymology": format!("Auto-generated etymology for '{}'", word.text),
            "roots": [],
            "generated": true,
            "generatedAt": chrono::Utc::now().to_rfc3339(),
        });

        let key = match crate::store::keys::etymology_key(&word.id) {
            Ok(k) => k,
            Err(e) => {
                tracing::warn!(word_id = %word.id, error = %e, "Failed to build etymology key");
                continue;
            }
        };
        let bytes = match serde_json::to_vec(&etymology) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(word_id = %word.id, error = %e, "Failed to serialize etymology");
                continue;
            }
        };
        if let Err(e) = store.etymologies.insert(key.as_bytes(), bytes) {
            tracing::warn!(word_id = %word.id, error = %e, "Failed to store etymology");
        }
    }

    tracing::info!(
        processed = words_to_process.len(),
        "Etymology generation complete"
    );
}
