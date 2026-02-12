// TODO: 实现词向量嵌入生成 worker。需要集成外部 embedding 服务（如 OpenAI embeddings），
// 为缺少嵌入的单词生成向量表示，存储到 Word.embedding 字段，用于语义搜索。
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
