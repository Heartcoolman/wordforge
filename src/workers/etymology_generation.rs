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

        let key = crate::store::keys::etymology_key(&word.id);
        if store.etymologies.get(key.as_bytes()).ok().flatten().is_none() {
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
            "etymology": format!("Auto-generated etymology for '{}'", word.text),
            "roots": [],
            "generated": true,
            "generatedAt": chrono::Utc::now().to_rfc3339(),
        });

        let key = crate::store::keys::etymology_key(&word.id);
        let bytes = match serde_json::to_vec(&etymology) {
            Ok(b) => b,
            Err(e) => {
                tracing::warn!(word_id = %word.id, error = %e, "Failed to serialize etymology");
                continue;
            }
        };
        if let Err(e) = store.etymologies.insert(
            key.as_bytes(),
            bytes,
        ) {
            tracing::warn!(word_id = %word.id, error = %e, "Failed to store etymology");
        }
    }

    tracing::info!(processed = words_to_process.len(), "Etymology generation complete");
}
