use crate::store::operations::records::{LearningRecord, UserStatsAgg};
use crate::store::operations::users::User;
use crate::store::operations::words::Word;
use crate::store::{keys, operations::word_states::WordLearningState};
use crate::store::{Store, StoreError};

const VERSION_KEY: &str = "_meta:version";

type MigrationFn = fn(&Store) -> Result<(), StoreError>;

fn migrations() -> Vec<(&'static str, MigrationFn)> {
    vec![
        ("001_initial", m001_initial),
        ("002_word_due_index", m002_word_due_index),
        ("003_secondary_indexes", m003_secondary_indexes),
        ("004_wordbook_type_index", m004_wordbook_type_index),
    ]
}

/// 执行所有未应用的数据库迁移。
///
/// 迁移设计原则：
/// - **幂等性要求**：每个迁移函数必须是幂等的，即重复执行不会产生副作用。
///   这是因为迁移可能在 func() 成功但 set_version() 之前因进程崩溃而中断，
///   重启后会重新执行该迁移。
/// - **进度检查点**：版本号在每个迁移成功后立即持久化（set_version），
///   确保已完成的迁移不会被重复执行。
/// - **仅向前**：set_version 拒绝降级，防止意外回滚。
pub fn run(store: &Store) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    let all = migrations();

    for (index, (name, func)) in all.iter().enumerate() {
        let version = (index + 1) as u32;
        if version > current {
            tracing::info!(version, name, "Running migration");
            func(store)?;
            set_version(store, version)?;
            tracing::info!(version, name, "Migration complete");
        } else {
            tracing::debug!(version, name, "Migration already applied, skipping");
        }
    }

    Ok(())
}

pub fn get_current_version(store: &Store) -> Result<u32, StoreError> {
    match store.config_versions.get(VERSION_KEY.as_bytes())? {
        Some(raw) => {
            if raw.len() == 4 {
                let bytes: [u8; 4] = raw.as_ref().try_into().unwrap_or([0; 4]);
                Ok(u32::from_be_bytes(bytes))
            } else {
                // Legacy string format fallback
                let text = String::from_utf8(raw.to_vec()).unwrap_or_else(|_| "0".to_string());
                Ok(text.parse::<u32>().unwrap_or(0))
            }
        }
        None => Ok(0),
    }
}

pub fn set_version(store: &Store, version: u32) -> Result<(), StoreError> {
    let current = get_current_version(store)?;
    if version < current {
        return Err(StoreError::Migration {
            version,
            message: format!("Refuse to downgrade from {} to {}", current, version),
        });
    }

    store
        .config_versions
        .insert(VERSION_KEY.as_bytes(), &version.to_be_bytes())?;
    Ok(())
}

fn m001_initial(_store: &Store) -> Result<(), StoreError> {
    Ok(())
}

fn m002_word_due_index(store: &Store) -> Result<(), StoreError> {
    for item in store.word_learning_states.iter() {
        let (_, value) = item?;
        let state: WordLearningState = Store::deserialize(&value)?;

        if let Some(next_review_date) = state.next_review_date {
            let due_index_key = keys::word_due_index_key(
                &state.user_id,
                next_review_date.timestamp_millis(),
                &state.word_id,
            )?;
            store.word_due_index.insert(due_index_key.as_bytes(), &[])?;
        }
    }

    Ok(())
}

/// Build all secondary indexes for existing data.
fn m003_secondary_indexes(store: &Store) -> Result<(), StoreError> {
    // 1. users_by_created_at index
    for item in store.users.iter() {
        let (key, value) = item?;
        if key.starts_with(b"email:") {
            continue;
        }
        if let Ok(user) = Store::deserialize::<User>(&value) {
            let idx_key = keys::users_by_created_at_key(
                user.created_at.timestamp_millis(),
                &user.id,
            )?;
            store.users_by_created_at.insert(idx_key.as_bytes(), user.id.as_bytes())?;
        }
    }

    // 2. words_by_created_at index
    for item in store.words.iter() {
        let (_, value) = item?;
        if let Ok(word) = Store::deserialize::<Word>(&value) {
            let idx_key = keys::words_by_created_at_key(
                word.created_at.timestamp_millis(),
                &word.id,
            )?;
            store.words_by_created_at.insert(idx_key.as_bytes(), word.id.as_bytes())?;
        }
    }

    // 3. records_by_time index + user_stats aggregation + word_references for records
    let mut user_stats_map: std::collections::HashMap<String, UserStatsAgg> =
        std::collections::HashMap::new();

    for item in store.records.iter() {
        let (k, value) = item?;
        if let Ok(record) = Store::deserialize::<LearningRecord>(&value) {
            let ts = record.created_at.timestamp_millis();
            let time_key = keys::records_by_time_key(ts, &record.id)?;
            store.records_by_time.insert(time_key.as_bytes(), record.user_id.as_bytes())?;

            // word_references index
            let ref_key = keys::word_ref_key(&record.word_id, "records", &k)?;
            store.word_references.insert(ref_key.as_bytes(), &[])?;

            // Accumulate user stats
            let stats = user_stats_map.entry(record.user_id.clone()).or_default();
            stats.total_records += 1;
            if record.is_correct {
                stats.correct_records += 1;
            }
            stats.word_ids.insert(record.word_id.clone());
            if let Some(ref sid) = record.session_id {
                stats.session_ids.insert(sid.clone());
            }
        }
    }

    // Write user stats
    for (user_id, stats) in &user_stats_map {
        let key = keys::user_stats_key(user_id)?;
        store.user_stats.insert(key.as_bytes(), Store::serialize(stats)?)?;
    }

    // 4. word_references for wordbook_words
    for item in store.wordbook_words.iter() {
        let (k, value) = item?;
        if let Ok(entry) = Store::deserialize::<crate::store::operations::wordbooks::WordbookWordEntry>(&value) {
            let ref_key = keys::word_ref_key(&entry.word_id, "wordbook_words", &k)?;
            store.word_references.insert(ref_key.as_bytes(), &[])?;
        }
    }

    // 5. word_references for word_learning_states
    for item in store.word_learning_states.iter() {
        let (k, value) = item?;
        if let Ok(state) = Store::deserialize::<WordLearningState>(&value) {
            let ref_key = keys::word_ref_key(&state.word_id, "word_learning_states", &k)?;
            store.word_references.insert(ref_key.as_bytes(), &[])?;

            // Also index due_index entries
            if let Some(next_review_date) = state.next_review_date {
                let due_key = keys::word_due_index_key(
                    &state.user_id,
                    next_review_date.timestamp_millis(),
                    &state.word_id,
                )?;
                let ref_due_key = keys::word_ref_key(&state.word_id, "word_due_index", due_key.as_bytes())?;
                store.word_references.insert(ref_due_key.as_bytes(), &[])?;
            }
        }
    }

    Ok(())
}

/// Build wordbook_type_index for existing wordbooks.
fn m004_wordbook_type_index(store: &Store) -> Result<(), StoreError> {
    use crate::store::operations::wordbooks::{Wordbook, WordbookType};

    for item in store.wordbooks.iter() {
        let (_, value) = item?;
        if let Ok(book) = Store::deserialize::<Wordbook>(&value) {
            let idx_key = match book.book_type {
                WordbookType::System => keys::wordbook_type_index_key_system(&book.id)?,
                WordbookType::User => {
                    let uid = book.user_id.as_deref().unwrap_or("unknown");
                    keys::wordbook_type_index_key_user(uid, &book.id)?
                }
            };
            store
                .wordbook_type_index
                .insert(idx_key.as_bytes(), book.id.as_bytes())?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn migration_is_idempotent() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db");
        let store = Store::open(path.to_str().unwrap()).unwrap();

        run(&store).unwrap();
        let first = get_current_version(&store).unwrap();
        run(&store).unwrap();
        let second = get_current_version(&store).unwrap();

        assert_eq!(first, 4);
        assert_eq!(second, 4);
    }

    #[test]
    fn downgrade_is_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("db2");
        let store = Store::open(path.to_str().unwrap()).unwrap();

        set_version(&store, 3).unwrap();
        let err = set_version(&store, 2).unwrap_err();
        assert!(matches!(err, StoreError::Migration { .. }));
    }
}
