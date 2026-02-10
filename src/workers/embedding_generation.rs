use crate::store::Store;

pub async fn run(store: &Store) {
    tracing::debug!("Embedding generation worker tick");

    let words = match store.get_words_without_embedding(20) {
        Ok(w) => w,
        Err(e) => {
            tracing::warn!(error = %e, "Failed to get words without embeddings");
            return;
        }
    };

    if words.is_empty() {
        return;
    }

    tracing::info!(
        count = words.len(),
        "Found words without embeddings (embedding service integration pending)"
    );
}
