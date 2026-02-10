use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

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
        let key = keys::word_key(&word.id);
        self.words.insert(key.as_bytes(), Self::serialize(word)?)?;
        Ok(())
    }

    pub fn get_word(&self, word_id: &str) -> Result<Option<Word>, StoreError> {
        let key = keys::word_key(word_id);
        match self.words.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn list_words(&self, limit: usize, offset: usize) -> Result<Vec<Word>, StoreError> {
        let mut words = Vec::new();
        for item in self.words.iter() {
            let (_, v) = item?;
            words.push(Self::deserialize::<Word>(&v)?);
        }

        words.sort_by(|a, b| a.text.cmp(&b.text));
        Ok(words.into_iter().skip(offset).take(limit).collect())
    }

    pub fn delete_word(&self, word_id: &str) -> Result<(), StoreError> {
        let key = keys::word_key(word_id);
        self.words.remove(key.as_bytes())?;

        // Cascade: remove from wordbook_words entries containing this word_id
        let mut affected_wordbooks = Vec::new();
        for item in self.wordbook_words.iter() {
            let (k, v) = item?;
            let key_str = String::from_utf8_lossy(&k);
            // Keys are formatted as "{wordbook_id}:{word_id}"
            if key_str.ends_with(&format!(":{}", word_id)) {
                if let Ok(entry) = Self::deserialize::<crate::store::operations::wordbooks::WordbookWordEntry>(&v) {
                    affected_wordbooks.push(entry.wordbook_id.clone());
                }
                self.wordbook_words.remove(k.as_ref())?;
            }
        }

        // Update word counts for affected wordbooks
        for wb_id in affected_wordbooks {
            let wb_key = keys::wordbook_key(&wb_id);
            if let Some(raw) = self.wordbooks.get(wb_key.as_bytes())? {
                let mut book: crate::store::operations::wordbooks::Wordbook = Self::deserialize(&raw)?;
                book.word_count = self.count_wordbook_words(&wb_id)?;
                self.wordbooks.insert(wb_key.as_bytes(), Self::serialize(&book)?)?;
            }
        }

        // Cascade: remove word_learning_states entries for this word
        // Keys are formatted as "{user_id}:{word_id}"
        for item in self.word_learning_states.iter() {
            let (k, _) = item?;
            let key_str = String::from_utf8_lossy(&k);
            if key_str.ends_with(&format!(":{}", word_id)) {
                self.word_learning_states.remove(k.as_ref())?;
            }
        }

        // Cascade: remove records entries for this word
        // Records keys are "{user_id}:{reverse_ts}:{record_id}", scan for word_id in values
        for item in self.records.iter() {
            let (k, v) = item?;
            // Check if the record references this word_id by inspecting the value
            let value_str = String::from_utf8_lossy(&v);
            if value_str.contains(word_id) {
                // Deserialize to check properly
                if let Ok(val) = serde_json::from_slice::<serde_json::Value>(&v) {
                    if val.get("wordId").and_then(|v| v.as_str()) == Some(word_id)
                        || val.get("word_id").and_then(|v| v.as_str()) == Some(word_id)
                    {
                        self.records.remove(k.as_ref())?;
                    }
                }
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
}
