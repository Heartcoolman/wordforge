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
        // Maintain words_by_created_at index
        let idx_key = keys::words_by_created_at_key(
            word.created_at.timestamp_millis(),
            &word.id,
        )?;
        self.words_by_created_at
            .insert(idx_key.as_bytes(), word.id.as_bytes())?;
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
        // Use words_by_created_at index (reverse timestamp = newest first)
        if self.words_by_created_at.len() > 0 {
            let mut words = Vec::new();
            let mut skipped = 0usize;
            for item in self.words_by_created_at.iter() {
                let (_, value) = item?;
                let word_id = String::from_utf8(value.to_vec()).unwrap_or_default();
                if skipped < offset {
                    skipped += 1;
                    continue;
                }
                if let Some(word) = self.get_word(&word_id)? {
                    words.push(word);
                }
                if words.len() >= limit {
                    break;
                }
            }
            return Ok(words);
        }

        // Fallback: full scan (only if index not yet built)
        let mut words = Vec::new();
        for item in self.words.iter() {
            let (_, v) = item?;
            words.push(Self::deserialize::<Word>(&v)?);
        }

        words.sort_by(|a, b| a.text.cmp(&b.text));
        Ok(words.into_iter().skip(offset).take(limit).collect())
    }

    pub fn delete_word(&self, word_id: &str) -> Result<(), StoreError> {
        let word_key = keys::word_key(word_id)?;

        // Get word data before deletion for index cleanup
        let word_data = self.get_word(word_id)?;

        // Try to use word_references index for fast lookup
        let ref_prefix = keys::word_ref_prefix(word_id)?;
        let has_refs = self.word_references.scan_prefix(ref_prefix.as_bytes()).next().is_some();

        let mut ww_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        let mut affected_wordbook_ids: Vec<String> = Vec::new();
        let mut wls_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        let mut due_index_keys_to_remove: Vec<Vec<u8>> = Vec::new();
        let mut rec_keys_to_remove: Vec<Vec<u8>> = Vec::new();

        if has_refs {
            for item in self.word_references.scan_prefix(ref_prefix.as_bytes()) {
                let (ref_key, _) = item?;
                let ref_key_str = String::from_utf8_lossy(&ref_key);
                let parts: Vec<&str> = ref_key_str.splitn(3, ':').collect();
                if parts.len() < 3 {
                    continue;
                }
                let tree_name = parts[1];
                let assoc_key_hex = parts[2];
                let assoc_key = hex::decode(assoc_key_hex).unwrap_or_default();

                match tree_name {
                    "records" => rec_keys_to_remove.push(assoc_key),
                    "wordbook_words" => {
                        if let Some(raw) = self.wordbook_words.get(&assoc_key)? {
                            if let Ok(ww_entry) = Self::deserialize::<crate::store::operations::wordbooks::WordbookWordEntry>(&raw) {
                                affected_wordbook_ids.push(ww_entry.wordbook_id.clone());
                            }
                        }
                        ww_keys_to_remove.push(assoc_key);
                    }
                    "word_learning_states" => wls_keys_to_remove.push(assoc_key),
                    "word_due_index" => due_index_keys_to_remove.push(assoc_key),
                    _ => {}
                }
            }
        } else {
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
        }

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

        // Clean up words_by_created_at index
        if let Some(word) = word_data {
            if let Ok(idx_key) = keys::words_by_created_at_key(word.created_at.timestamp_millis(), word_id) {
                let _ = self.words_by_created_at.remove(idx_key.as_bytes());
            }
        }

        // Clean up word_references index
        for item in self.word_references.scan_prefix(ref_prefix.as_bytes()) {
            if let Ok((k, _)) = item {
                let _ = self.word_references.remove(&k);
            }
        }

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
        let texts: Vec<&str> = list.iter().map(|w| w.text.as_str()).collect();
        assert!(texts.contains(&"apple"));
        assert!(texts.contains(&"banana"));
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
