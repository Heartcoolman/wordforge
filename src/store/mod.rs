pub mod keys;
pub mod migrate;
pub mod operations;
pub mod trees;

use serde::de::DeserializeOwned;
use serde::Serialize;
use sled::Db;
use thiserror::Error;

#[derive(Debug)]
pub struct Store {
    db: Db,
    pub users: sled::Tree,
    pub sessions: sled::Tree,
    pub admin_sessions: sled::Tree,
    pub words: sled::Tree,
    pub records: sled::Tree,
    pub learning_sessions: sled::Tree,
    pub engine_user_states: sled::Tree,
    pub engine_algorithm_states: sled::Tree,
    pub engine_monitoring_events: sled::Tree,
    pub algorithm_metrics_daily: sled::Tree,
    pub password_reset_tokens: sled::Tree,
    pub config_versions: sled::Tree,
    // P0 new trees
    pub admins: sled::Tree,
    pub wordbooks: sled::Tree,
    pub wordbook_words: sled::Tree,
    pub word_learning_states: sled::Tree,
    pub word_due_index: sled::Tree,
    pub study_configs: sled::Tree,
    // P4 trees
    pub user_profiles: sled::Tree,
    pub habit_profiles: sled::Tree,
    pub notifications: sled::Tree,
    pub badges: sled::Tree,
    pub user_preferences: sled::Tree,
    pub etymologies: sled::Tree,
    pub word_morphemes: sled::Tree,
    pub confusion_pairs: sled::Tree,
    pub wb_center_imports: sled::Tree,
    // Secondary index trees
    pub users_by_created_at: sled::Tree,
    pub words_by_created_at: sled::Tree,
    pub records_by_time: sled::Tree,
    pub word_references: sled::Tree,
    pub user_stats: sled::Tree,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sled error: {0}")]
    Sled(#[from] sled::Error),
    #[error("serialization error: {0}")]
    Serialization(#[from] serde_json::Error),
    #[error("not found: entity={entity}, key={key}")]
    NotFound { entity: String, key: String },
    #[error("conflict: entity={entity}, key={key}")]
    Conflict { entity: String, key: String },
    #[error("CAS retry exhausted after {attempts} attempts: entity={entity}, key={key}")]
    CasRetryExhausted {
        entity: String,
        key: String,
        attempts: u32,
    },
    #[error("validation error: {0}")]
    Validation(String),
    #[error("migration error at version {version}: {message}")]
    Migration { version: u32, message: String },
}

impl Store {
    pub fn open(sled_path: &str) -> Result<Self, StoreError> {
        let db = sled::open(sled_path)?;
        let users = db.open_tree(trees::USERS)?;
        let sessions = db.open_tree(trees::SESSIONS)?;
        let admin_sessions = db.open_tree(trees::ADMIN_SESSIONS)?;
        let words = db.open_tree(trees::WORDS)?;
        let records = db.open_tree(trees::RECORDS)?;
        let learning_sessions = db.open_tree(trees::LEARNING_SESSIONS)?;
        let engine_user_states = db.open_tree(trees::ENGINE_USER_STATES)?;
        let engine_algorithm_states = db.open_tree(trees::ENGINE_ALGORITHM_STATES)?;
        let engine_monitoring_events = db.open_tree(trees::ENGINE_MONITORING_EVENTS)?;
        let algorithm_metrics_daily = db.open_tree(trees::ALGORITHM_METRICS_DAILY)?;
        let password_reset_tokens = db.open_tree(trees::PASSWORD_RESET_TOKENS)?;
        let config_versions = db.open_tree(trees::CONFIG_VERSIONS)?;
        // P0 new trees
        let admins = db.open_tree(trees::ADMINS)?;
        let wordbooks = db.open_tree(trees::WORDBOOKS)?;
        let wordbook_words = db.open_tree(trees::WORDBOOK_WORDS)?;
        let word_learning_states = db.open_tree(trees::WORD_LEARNING_STATES)?;
        let word_due_index = db.open_tree(trees::WORD_DUE_INDEX)?;
        let study_configs = db.open_tree(trees::STUDY_CONFIGS)?;
        // P4 trees
        let user_profiles = db.open_tree(trees::USER_PROFILES)?;
        let habit_profiles = db.open_tree(trees::HABIT_PROFILES)?;
        let notifications = db.open_tree(trees::NOTIFICATIONS)?;
        let badges = db.open_tree(trees::BADGES)?;
        let user_preferences = db.open_tree(trees::USER_PREFERENCES)?;
        let etymologies = db.open_tree(trees::ETYMOLOGIES)?;
        let word_morphemes = db.open_tree(trees::WORD_MORPHEMES)?;
        let confusion_pairs = db.open_tree(trees::CONFUSION_PAIRS)?;
        let wb_center_imports = db.open_tree(trees::WB_CENTER_IMPORTS)?;
        // Secondary index trees
        let users_by_created_at = db.open_tree(trees::USERS_BY_CREATED_AT)?;
        let words_by_created_at = db.open_tree(trees::WORDS_BY_CREATED_AT)?;
        let records_by_time = db.open_tree(trees::RECORDS_BY_TIME)?;
        let word_references = db.open_tree(trees::WORD_REFERENCES)?;
        let user_stats = db.open_tree(trees::USER_STATS)?;

        Ok(Self {
            db,
            users,
            sessions,
            admin_sessions,
            words,
            records,
            learning_sessions,
            engine_user_states,
            engine_algorithm_states,
            engine_monitoring_events,
            algorithm_metrics_daily,
            password_reset_tokens,
            config_versions,
            admins,
            wordbooks,
            wordbook_words,
            word_learning_states,
            word_due_index,
            study_configs,
            user_profiles,
            habit_profiles,
            notifications,
            badges,
            user_preferences,
            etymologies,
            word_morphemes,
            confusion_pairs,
            wb_center_imports,
            users_by_created_at,
            words_by_created_at,
            records_by_time,
            word_references,
            user_stats,
        })
    }

    pub fn run_migrations(&self) -> Result<(), StoreError> {
        migrate::run(self)
    }

    pub fn flush(&self) -> Result<(), StoreError> {
        self.db.flush()?;
        Ok(())
    }

    pub fn raw_db(&self) -> &Db {
        &self.db
    }

    pub(crate) fn serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, StoreError> {
        Ok(serde_json::to_vec(value)?)
    }

    pub(crate) fn deserialize<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, StoreError> {
        Ok(serde_json::from_slice(bytes)?)
    }
}
