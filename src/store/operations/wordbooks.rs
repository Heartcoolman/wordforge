use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::keys;
use crate::store::{Store, StoreError};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Wordbook {
    pub id: String,
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub book_type: WordbookType,
    pub user_id: Option<String>,
    pub word_count: u64,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum WordbookType {
    System,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WordbookWordEntry {
    pub wordbook_id: String,
    pub word_id: String,
    pub added_at: DateTime<Utc>,
}

impl Store {
    pub fn upsert_wordbook(&self, wordbook: &Wordbook) -> Result<(), StoreError> {
        let key = keys::wordbook_key(&wordbook.id);
        self.wordbooks
            .insert(key.as_bytes(), Self::serialize(wordbook)?)?;
        Ok(())
    }

    pub fn get_wordbook(&self, wordbook_id: &str) -> Result<Option<Wordbook>, StoreError> {
        let key = keys::wordbook_key(wordbook_id);
        match self.wordbooks.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    pub fn list_system_wordbooks(&self) -> Result<Vec<Wordbook>, StoreError> {
        let mut books = Vec::new();
        for item in self.wordbooks.iter() {
            let (_, v) = item?;
            let book: Wordbook = Self::deserialize(&v)?;
            if book.book_type == WordbookType::System {
                books.push(book);
            }
        }
        books.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(books)
    }

    pub fn list_user_wordbooks(&self, user_id: &str) -> Result<Vec<Wordbook>, StoreError> {
        let mut books = Vec::new();
        for item in self.wordbooks.iter() {
            let (_, v) = item?;
            let book: Wordbook = Self::deserialize(&v)?;
            if book.book_type == WordbookType::User
                && book.user_id.as_deref() == Some(user_id)
            {
                books.push(book);
            }
        }
        books.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(books)
    }

    pub fn add_word_to_wordbook(
        &self,
        wordbook_id: &str,
        word_id: &str,
    ) -> Result<(), StoreError> {
        let ww_key = keys::wordbook_words_key(wordbook_id, word_id);
        let entry = WordbookWordEntry {
            wordbook_id: wordbook_id.to_string(),
            word_id: word_id.to_string(),
            added_at: Utc::now(),
        };
        let entry_bytes = Self::serialize(&entry)?;

        // Insert the word entry
        self.wordbook_words
            .insert(ww_key.as_bytes(), entry_bytes)?;

        // Recount AFTER the insert has succeeded, then update the wordbook
        let count = self.count_wordbook_words(wordbook_id)?;
        let wb_key = keys::wordbook_key(wordbook_id);
        if let Some(raw) = self.wordbooks.get(wb_key.as_bytes())? {
            let mut book: Wordbook = Self::deserialize(&raw)?;
            book.word_count = count;
            self.wordbooks
                .insert(wb_key.as_bytes(), Self::serialize(&book)?)?;
        }
        Ok(())
    }

    pub fn remove_word_from_wordbook(
        &self,
        wordbook_id: &str,
        word_id: &str,
    ) -> Result<(), StoreError> {
        let ww_key = keys::wordbook_words_key(wordbook_id, word_id);
        self.wordbook_words.remove(ww_key.as_bytes())?;

        // Recount AFTER the remove has succeeded, then update the wordbook
        let count = self.count_wordbook_words(wordbook_id)?;
        let wb_key = keys::wordbook_key(wordbook_id);
        if let Some(raw) = self.wordbooks.get(wb_key.as_bytes())? {
            let mut book: Wordbook = Self::deserialize(&raw)?;
            book.word_count = count;
            self.wordbooks
                .insert(wb_key.as_bytes(), Self::serialize(&book)?)?;
        }
        Ok(())
    }

    pub fn list_wordbook_words(
        &self,
        wordbook_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<String>, StoreError> {
        let prefix = keys::wordbook_words_prefix(wordbook_id);
        let mut word_ids = Vec::new();
        for item in self.wordbook_words.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item?;
            let entry: WordbookWordEntry = Self::deserialize(&v)?;
            word_ids.push(entry.word_id);
        }
        Ok(word_ids.into_iter().skip(offset).take(limit).collect())
    }

    pub fn count_wordbook_words(&self, wordbook_id: &str) -> Result<u64, StoreError> {
        let prefix = keys::wordbook_words_prefix(wordbook_id);
        let mut count = 0u64;
        for item in self.wordbook_words.scan_prefix(prefix.as_bytes()) {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }
}
