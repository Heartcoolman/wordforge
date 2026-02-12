use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sled::Transactional;
use std::collections::HashMap;

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Word {
    pub id: String,
    pub text: String,
    pub meaning: String,
    pub pronunciation: Option<String>,
    pub part_of_speech: Option<String>,
    pub difficulty: f64,
    pub examples: Vec<String>,
    pub tags: Vec<String>,
    pub embedding: Option<Vec<f64>>,
    pub created_at: DateTime<Utc>,
}

impl Store {
    pub fn upsert_word(&self, word: &Word) -> Result<(), StoreError> {
        let key = keys::word_key(&word.id)?;
        self.words.insert(key.as_bytes(), Self::serialize(word)?)?;
        Ok(())
    }

    pub fn get_word(&self, word_id: &str) -> Result<Option<Word>, StoreError> {
        let key = keys::word_key(word_id)?;
        match self.words.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    /// 批量获取单词信息（仅返回存在的单词）
    pub fn get_words_by_ids(
        &self,
        word_ids: &[String],
    ) -> Result<HashMap<String, Word>, StoreError> {
        let mut words = HashMap::with_capacity(word_ids.len());

        for word_id in word_ids {
            if words.contains_key(word_id) {
                continue;
            }

            if let Some(word) = self.get_word(word_id)? {
                words.insert(word_id.clone(), word);
            }
        }

        Ok(words)
    }

    pub fn list_words(&self, limit: usize, offset: usize) -> Result<Vec<Word>, StoreError> {
        // 注意：Sled 按字节序存储 key（word_id），而非按 text 排序。
        // 如果不需要按 text 排序，可以直接用 .skip(offset).take(limit) 在迭代器上分页。
        // TODO: 引入 text 前缀索引来实现按 text 有序的前缀扫描分页。
        let mut words = Vec::new();
        for item in self.words.iter() {
            let (_, v) = item?;
            words.push(Self::deserialize::<Word>(&v)?);
        }

        words.sort_by(|a, b| a.text.cmp(&b.text));
        Ok(words.into_iter().skip(offset).take(limit).collect())
    }

    pub fn delete_word(&self, word_id: &str) -> Result<(), StoreError> {
        // Phase 1: Collect keys to delete (cannot iterate inside a transaction)
        let word_key = keys::word_key(word_id)?;

        // Collect wordbook_words keys and affected wordbook IDs
        // TODO: 添加 word_id -> wordbook_id 的反向索引，避免全表扫描 wordbook_words
        let mut ww_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        let mut affected_wordbook_ids: Vec<String> = Vec::new();
        let suffix = format!(":{}", word_id);
        for item in self.wordbook_words.iter() {
            let (k, v) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if key_str.ends_with(&suffix) {
                if let Ok(entry) =
                    Self::deserialize::<crate::store::operations::wordbooks::WordbookWordEntry>(&v)
                {
                    affected_wordbook_ids.push(entry.wordbook_id.clone());
                }
                ww_keys_to_remove.push(k.to_vec());
            }
        }

        // Collect word_learning_states keys
        // TODO: 添加 word_id -> user_id 的反向索引，避免全表扫描 word_learning_states
        let mut wls_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        let mut due_index_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        for item in self.word_learning_states.iter() {
            let (k, _) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if key_str.ends_with(&suffix) {
                wls_keys_to_remove.push(k.to_vec());
            }
        }

        for item in self.word_due_index.iter() {
            let (k, _) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if key_str.ends_with(&suffix) {
                due_index_keys_to_remove.push(k.to_vec());
            }
        }

        // Collect records keys (only check "wordId" camelCase since serde uses rename_all)
        // TODO: 添加 word_id -> record_key 的反向索引，避免全表扫描 records
        let mut rec_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        for item in self.records.iter() {
            let (k, v) = item?;
            let value_str = String::from_utf8_lossy(&v);
            if value_str.contains(word_id) {
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&v) {
                    if val.get("wordId").and_then(|v| v.as_str()) == Some(word_id) {
                        rec_keys_to_remove.push(k.to_vec());
                    }
                }
            }
        }

        // Pre-compute updated wordbook data (decrement counts instead of full recount)
        let mut wordbook_updates: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
        for wb_id in &affected_wordbook_ids {
            let wb_key = keys::wordbook_key(wb_id)?;
            if let Some(raw) = self.wordbooks.get(wb_key.as_bytes())? {
                let mut book: crate::store::operations::wordbooks::Wordbook =
                    Self::deserialize(&raw)?;
                book.word_count = book.word_count.saturating_sub(1);
                wordbook_updates.push((wb_key.into_bytes(), Self::serialize(&book)?));
            }
        }

        // Phase 2: Execute all mutations atomically in a transaction
        (
            &self.words,
            &self.wordbook_words,
            &self.word_learning_states,
            &self.word_due_index,
            &self.records,
            &self.wordbooks,
        )
            .transaction(|(tx_words, tx_ww, tx_wls, tx_due, tx_rec, tx_wb)| {
                tx_words.remove(word_key.as_bytes())?;

                for k in &ww_keys_to_remove {
                    tx_ww.remove(k.as_slice())?;
                }

                for k in &wls_keys_to_remove {
                    tx_wls.remove(k.as_slice())?;
                }

                for k in &due_index_keys_to_remove {
                    tx_due.remove(k.as_slice())?;
                }

                for k in &rec_keys_to_remove {
                    tx_rec.remove(k.as_slice())?;
                }

                for (wb_key, wb_bytes) in &wordbook_updates {
                    tx_wb.insert(wb_key.as_slice(), wb_bytes.as_slice())?;
                }

                Ok(())
            })
            .map_err(
                |e: sled::transaction::TransactionError<StoreError>| match e {
                    sled::transaction::TransactionError::Abort(store_err) => store_err,
                    sled::transaction::TransactionError::Storage(sled_err) => {
                        StoreError::Sled(sled_err)
                    }
                },
            )?;

        Ok(())
    }

    pub fn count_words(&self) -> Result<u64, StoreError> {
        Ok(self.words.len() as u64)
    }

    pub fn search_words(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<(Vec<Word>, u64), StoreError> {
        // 注意：文本搜索需要遍历所有单词来匹配，无法使用前缀扫描优化。
        // TODO: 引入全文搜索索引（如倒排索引）来避免全表扫描。
        let query_lower = query.to_lowercase();
        let mut matching = Vec::new();
        for item in self.words.iter() {
            let (_, v) = item?;
            let word: Word = Self::deserialize(&v)?;
            if word.text.to_lowercase().contains(&query_lower)
                || word.meaning.to_lowercase().contains(&query_lower)
            {
                matching.push(word);
            }
        }
        matching.sort_by(|a, b| a.text.cmp(&b.text));
        let total = matching.len() as u64;
        let items = matching.into_iter().skip(offset).take(limit).collect();
        Ok((items, total))
    }

    pub fn get_words_without_embedding(&self, limit: usize) -> Result<Vec<Word>, StoreError> {
        let mut words = Vec::new();
        for item in self.words.iter() {
            let (_, v) = item?;
            let word: Word = Self::deserialize(&v)?;
            if word.embedding.is_none() {
                words.push(word);
            }
            if words.len() >= limit {
                break;
            }
        }
        Ok(words)
    }
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    fn sample_word(id: &str, text: &str) -> Word {
        Word {
            id: id.to_string(),
            text: text.to_string(),
            meaning: "meaning".to_string(),
            pronunciation: None,
            part_of_speech: None,
            difficulty: 0.5,
            examples: vec!["ex".to_string()],
            tags: vec!["tag".to_string()],
            embedding: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn create_and_list_words() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("words-db");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();

        let list = store.list_words(10, 0).unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].text, "apple");
    }

    #[test]
    fn get_words_by_ids_returns_existing_words_only() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("words-db-batch");
        let store = Store::open(db_path.to_str().unwrap()).unwrap();

        store.upsert_word(&sample_word("w1", "apple")).unwrap();
        store.upsert_word(&sample_word("w2", "banana")).unwrap();

        let words = store
            .get_words_by_ids(&[
                "w2".to_string(),
                "missing".to_string(),
                "w1".to_string(),
                "w1".to_string(),
            ])
            .unwrap();

        assert_eq!(words.len(), 2);
        assert!(words.contains_key("w1"));
        assert!(words.contains_key("w2"));
    }
}
