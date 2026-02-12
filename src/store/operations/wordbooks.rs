use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sled::Transactional;

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
        let key = keys::wordbook_key(&wordbook.id)?;
        self.wordbooks
            .insert(key.as_bytes(), Self::serialize(wordbook)?)?;
        Ok(())
    }

    pub fn get_wordbook(&self, wordbook_id: &str) -> Result<Option<Wordbook>, StoreError> {
        let key = keys::wordbook_key(wordbook_id)?;
        match self.wordbooks.get(key.as_bytes())? {
            Some(raw) => Ok(Some(Self::deserialize(&raw)?)),
            None => Ok(None),
        }
    }

    /// 列出所有系统词书。
    /// 注意：wordbook key 为纯 wordbook_id，没有按类型分类的前缀，
    /// 因此无法使用 Sled 前缀扫描来过滤。
    /// TODO: 引入类型前缀索引（如 `system:{wordbook_id}` / `user:{user_id}:{wordbook_id}`），
    /// 以支持按类型的高效前缀扫描。
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

    /// 列出指定用户的词书。
    /// TODO: 同 list_system_wordbooks，需要类型前缀索引来避免全表扫描。
    pub fn list_user_wordbooks(&self, user_id: &str) -> Result<Vec<Wordbook>, StoreError> {
        let mut books = Vec::new();
        for item in self.wordbooks.iter() {
            let (_, v) = item?;
            let book: Wordbook = Self::deserialize(&v)?;
            if book.book_type == WordbookType::User && book.user_id.as_deref() == Some(user_id) {
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
    ) -> Result<bool, StoreError> {
        let ww_key = keys::wordbook_words_key(wordbook_id, word_id)?;
        let wordbook_id_owned = wordbook_id.to_string();
        let entry = WordbookWordEntry {
            wordbook_id: wordbook_id.to_string(),
            word_id: word_id.to_string(),
            added_at: Utc::now(),
        };
        let entry_bytes = Self::serialize(&entry)?;
        let wb_key = keys::wordbook_key(wordbook_id)?;

        (&self.wordbook_words, &self.wordbooks)
            .transaction(|(tx_ww, tx_wb)| {
                let raw = tx_wb.get(wb_key.as_bytes())?.ok_or_else(|| {
                    sled::transaction::ConflictableTransactionError::Abort(StoreError::NotFound {
                        entity: "wordbook".to_string(),
                        key: wordbook_id_owned.clone(),
                    })
                })?;
                let mut book: Wordbook = serde_json::from_slice(&raw).map_err(|e| {
                    sled::transaction::ConflictableTransactionError::Abort(
                        StoreError::Serialization(e),
                    )
                })?;

                let inserted_new = tx_ww
                    .insert(ww_key.as_bytes(), entry_bytes.as_slice())?
                    .is_none();

                if inserted_new {
                    book.word_count = book.word_count.saturating_add(1);
                    let book_bytes = serde_json::to_vec(&book).map_err(|e| {
                        sled::transaction::ConflictableTransactionError::Abort(
                            StoreError::Serialization(e),
                        )
                    })?;
                    tx_wb.insert(wb_key.as_bytes(), book_bytes)?;
                }

                Ok(inserted_new)
            })
            .map_err(
                |e: sled::transaction::TransactionError<StoreError>| match e {
                    sled::transaction::TransactionError::Abort(store_err) => store_err,
                    sled::transaction::TransactionError::Storage(sled_err) => {
                        StoreError::Sled(sled_err)
                    }
                },
            )
    }

    pub fn remove_word_from_wordbook(
        &self,
        wordbook_id: &str,
        word_id: &str,
    ) -> Result<bool, StoreError> {
        let ww_key = keys::wordbook_words_key(wordbook_id, word_id)?;
        let wb_key = keys::wordbook_key(wordbook_id)?;

        (&self.wordbook_words, &self.wordbooks)
            .transaction(|(tx_ww, tx_wb)| {
                let removed_existing = tx_ww.remove(ww_key.as_bytes())?.is_some();

                if removed_existing {
                    if let Some(raw) = tx_wb.get(wb_key.as_bytes())? {
                        let mut book: Wordbook = serde_json::from_slice(&raw).map_err(|e| {
                            sled::transaction::ConflictableTransactionError::Abort(
                                StoreError::Serialization(e),
                            )
                        })?;
                        book.word_count = book.word_count.saturating_sub(1);
                        let book_bytes = serde_json::to_vec(&book).map_err(|e| {
                            sled::transaction::ConflictableTransactionError::Abort(
                                StoreError::Serialization(e),
                            )
                        })?;
                        tx_wb.insert(wb_key.as_bytes(), book_bytes)?;
                    }
                }

                Ok(removed_existing)
            })
            .map_err(
                |e: sled::transaction::TransactionError<StoreError>| match e {
                    sled::transaction::TransactionError::Abort(store_err) => store_err,
                    sled::transaction::TransactionError::Storage(sled_err) => {
                        StoreError::Sled(sled_err)
                    }
                },
            )
    }

    pub fn list_wordbook_words(
        &self,
        wordbook_id: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<String>, StoreError> {
        let prefix = keys::wordbook_words_prefix(wordbook_id)?;
        let mut word_ids = Vec::new();
        let mut skipped = 0usize;
        for item in self.wordbook_words.scan_prefix(prefix.as_bytes()) {
            let (_, v) = item?;
            if skipped < offset {
                skipped += 1;
                continue;
            }
            let entry: WordbookWordEntry = Self::deserialize(&v)?;
            word_ids.push(entry.word_id);
            if word_ids.len() >= limit {
                break;
            }
        }
        Ok(word_ids)
    }

    pub fn count_wordbook_words(&self, wordbook_id: &str) -> Result<u64, StoreError> {
        let prefix = keys::wordbook_words_prefix(wordbook_id)?;
        let mut count = 0u64;
        for item in self.wordbook_words.scan_prefix(prefix.as_bytes()) {
            let _ = item?;
            count += 1;
        }
        Ok(count)
    }
}
