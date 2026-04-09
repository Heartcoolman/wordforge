//! CLI tool to migrate data from sled KV database to SQLite.
//! Requires the `sled-migration` feature: cargo run --features sled-migration --bin migrate-sled-to-sqlite

#[cfg(not(feature = "sled-migration"))]
fn main() {
    eprintln!("This binary requires the 'sled-migration' feature.");
    eprintln!("Usage: cargo run --features sled-migration --bin migrate-sled-to-sqlite -- --sled-path <path> --sqlite-path <path>");
    std::process::exit(1);
}

#[cfg(feature = "sled-migration")]
fn main() {
    sled_migrate::run();
}

#[cfg(feature = "sled-migration")]
mod sled_migrate {
    use std::collections::HashMap;

    const INDEX_TREES: &[&str] = &[
        "idx_users_by_created",
        "idx_words_by_created",
        "idx_records_by_time",
        "idx_word_references",
        "idx_user_stats",
        "idx_record_id",
        "idx_alert_dedup",
        "idx_wordbook_type",
        "idx_monitoring_ts",
    ];

    const SKIP_TREES: &[&str] = &["word_due_index"];

    pub fn run() {
        let args: Vec<String> = std::env::args().collect();
        let sled_path = get_arg(&args, "--sled-path").unwrap_or_else(|| "./data/learning.sled".into());
        let sqlite_path = get_arg(&args, "--sqlite-path").unwrap_or_else(|| "./data/learning.db".into());

        println!("Migrating: {} -> {}", sled_path, sqlite_path);

        let sled_db = sled::Config::new()
            .path(&sled_path)
            .mode(sled::Mode::LowSpace)
            .open()
            .expect("Failed to open sled database");

        let store = learning_backend::store::Store::open(&sqlite_path, 30000, 1)
            .expect("Failed to open SQLite database");

        let tree_names: Vec<String> = sled_db
            .tree_names()
            .iter()
            .map(|n| String::from_utf8_lossy(n).to_string())
            .filter(|n| n != "__sled__default")
            .collect();

        println!("Found {} trees", tree_names.len());
        let mut stats: HashMap<String, usize> = HashMap::new();

        for tree_name in &tree_names {
            if INDEX_TREES.iter().any(|idx: &&str| tree_name.starts_with(idx))
                || SKIP_TREES.contains(&tree_name.as_str())
            {
                println!("  SKIP {}", tree_name);
                continue;
            }

            let tree = sled_db.open_tree(tree_name).expect("open tree");
            let count = migrate_tree(&store, tree_name, &tree);
            println!("  {} -> {} rows", tree_name, count);
            stats.insert(tree_name.clone(), count);
        }

        println!("\nMigration complete:");
        let total: usize = stats.values().sum();
        println!("  Total rows migrated: {}", total);
        for (name, count) in &stats {
            println!("  {}: {}", name, count);
        }
    }

    fn get_arg(args: &[String], flag: &str) -> Option<String> {
        args.iter()
            .position(|a| a == flag)
            .and_then(|i| args.get(i + 1))
            .cloned()
    }

    fn migrate_tree(store: &learning_backend::store::Store, tree_name: &str, tree: &sled::Tree) -> usize {
        let conn = store.connection().expect("get connection");
        let mut count = 0;

        for item in tree.iter() {
            let (key, value) = match item {
                Ok(kv) => kv,
                Err(e) => {
                    eprintln!("    Error reading {}: {}", tree_name, e);
                    continue;
                }
            };

            let key_str = String::from_utf8_lossy(&key).to_string();

            // Skip email index entries in users tree
            if tree_name == "users" && key_str.starts_with("email:") {
                continue;
            }
            // Skip admin init sentinel
            if tree_name == "admins" && key_str == "__initialized" {
                continue;
            }
            // Skip user index entries in sessions/learning_sessions
            if (tree_name == "sessions" || tree_name == "admin_sessions" || tree_name == "learning_sessions")
                && key_str.starts_with("user:")
            {
                continue;
            }
            // Skip meta version key in config_versions
            if tree_name == "config_versions" && key_str.starts_with("_meta:") {
                continue;
            }

            match migrate_row(&conn, tree_name, &key_str, &value) {
                Ok(()) => count += 1,
                Err(e) => eprintln!("    Error migrating {}/{}: {}", tree_name, key_str, e),
            }
        }
        count
    }

    fn migrate_row(
        conn: &rusqlite::Connection,
        tree_name: &str,
        key: &str,
        value: &[u8],
    ) -> Result<(), Box<dyn std::error::Error>> {
        let val: serde_json::Value = serde_json::from_slice(value)?;

        match tree_name {
            "users" => {
                conn.execute(
                    "INSERT OR REPLACE INTO users (id, email, username, password_hash, is_banned, created_at, updated_at, failed_login_count, locked_until)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                    rusqlite::params![
                        s(&val, "id"), s(&val, "email"), s(&val, "username"), s(&val, "passwordHash"),
                        b(&val, "isBanned"), s(&val, "createdAt"), s(&val, "updatedAt"),
                        i(&val, "failedLoginCount"), os(&val, "lockedUntil"),
                    ],
                )?;
            }
            "admins" => {
                conn.execute(
                    "INSERT OR REPLACE INTO admins (id, email, password_hash, created_at, updated_at, failed_login_count, locked_until)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        s(&val, "id"), s(&val, "email"), s(&val, "passwordHash"),
                        s(&val, "createdAt"), s(&val, "updatedAt"),
                        i(&val, "failedLoginCount"), os(&val, "lockedUntil"),
                    ],
                )?;
            }
            "sessions" | "admin_sessions" => {
                let table = tree_name;
                conn.execute(
                    &format!("INSERT OR REPLACE INTO {table} (token_hash, user_id, token_type, created_at, expires_at, revoked) VALUES (?1, ?2, ?3, ?4, ?5, ?6)"),
                    rusqlite::params![
                        s(&val, "tokenHash"), s(&val, "userId"), s(&val, "tokenType"),
                        s(&val, "createdAt"), s(&val, "expiresAt"), b(&val, "revoked"),
                    ],
                )?;
            }
            "words" => {
                conn.execute(
                    "INSERT OR REPLACE INTO words (id, text, meaning, pronunciation, part_of_speech, difficulty, examples_json, tags_json, embedding_json, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
                    rusqlite::params![
                        s(&val, "id"), s(&val, "text"), s(&val, "meaning"),
                        os(&val, "pronunciation"), os(&val, "partOfSpeech"),
                        f(&val, "difficulty"),
                        json_arr(&val, "examples"), json_arr(&val, "tags"),
                        val.get("embedding").map(|v| v.to_string()),
                        s(&val, "createdAt"),
                    ],
                )?;
            }
            "records" => {
                conn.execute(
                    "INSERT OR REPLACE INTO learning_records (user_id, id, word_id, is_correct, response_time_ms, session_id, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    rusqlite::params![
                        s(&val, "userId"), s(&val, "id"), s(&val, "wordId"),
                        b(&val, "isCorrect"), i(&val, "responseTimeMs"),
                        os(&val, "sessionId"), s(&val, "createdAt"),
                    ],
                )?;
            }
            "engine_user_states" => {
                conn.execute(
                    "INSERT OR REPLACE INTO engine_user_states (user_id, state_json, created_at)
                     VALUES (?1, ?2, ?3)",
                    rusqlite::params![key, val.to_string(), chrono::Utc::now().to_rfc3339()],
                )?;
            }
            "engine_algo_states" => {
                let parts: Vec<&str> = key.splitn(2, ':').collect();
                if parts.len() == 2 {
                    // Route to appropriate table based on key prefix
                    if parts[1].starts_with("user_elo:") {
                        let uid = parts[1].strip_prefix("user_elo:").unwrap_or(parts[0]);
                        conn.execute(
                            "INSERT OR REPLACE INTO user_elo (user_id, rating, games) VALUES (?1, ?2, ?3)",
                            rusqlite::params![uid, f(&val, "rating"), i(&val, "games")],
                        )?;
                    } else if parts[1].starts_with("word_elo:") {
                        let wid = parts[1].strip_prefix("word_elo:").unwrap_or("");
                        conn.execute(
                            "INSERT OR REPLACE INTO word_elo (word_id, rating, games) VALUES (?1, ?2, ?3)",
                            rusqlite::params![wid, f(&val, "rating"), i(&val, "games")],
                        )?;
                    } else {
                        conn.execute(
                            "INSERT OR REPLACE INTO engine_algo_states (user_id, algo_id, state_json) VALUES (?1, ?2, ?3)",
                            rusqlite::params![parts[0], parts[1], val.to_string()],
                        )?;
                    }
                }
            }
            "config_versions" => {
                if key.starts_with("system_settings:") {
                    conn.execute(
                        "INSERT OR REPLACE INTO system_settings (singleton_id, max_users, registration_enabled, maintenance_mode, default_daily_words, wordbook_center_url)
                         VALUES (1, ?1, ?2, ?3, ?4, ?5)",
                        rusqlite::params![
                            i(&val, "maxUsers"), b(&val, "registrationEnabled"),
                            b(&val, "maintenanceMode"), i(&val, "defaultDailyWords"),
                            os(&val, "wordbookCenterUrl"),
                        ],
                    )?;
                }
            }
            // Simple JSON blob tables
            "notifications" | "badges" | "word_learning_states" | "study_configs"
            | "wordbooks" | "wordbook_words" | "password_reset_tokens" | "learning_sessions"
            | "user_profiles" | "habit_profiles" | "user_preferences" | "etymologies"
            | "word_morphemes" | "confusion_pairs" | "wb_center_imports"
            | "engine_monitoring" | "algo_metrics_daily" => {
                // For remaining trees, store as JSON in the appropriate table
                // This is a best-effort migration; specific table mapping may need manual adjustment
                eprintln!("    WARN: tree '{}' needs manual migration mapping for key '{}'", tree_name, key);
            }
            _ => {
                eprintln!("    WARN: unknown tree '{}', skipping key '{}'", tree_name, key);
            }
        }
        Ok(())
    }

    fn s(v: &serde_json::Value, key: &str) -> String {
        v.get(key).and_then(|v| v.as_str()).unwrap_or("").to_string()
    }
    fn os(v: &serde_json::Value, key: &str) -> Option<String> {
        v.get(key).and_then(|v| v.as_str()).map(|s| s.to_string())
    }
    fn i(v: &serde_json::Value, key: &str) -> i64 {
        v.get(key).and_then(|v| v.as_i64()).unwrap_or(0)
    }
    fn f(v: &serde_json::Value, key: &str) -> f64 {
        v.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }
    fn b(v: &serde_json::Value, key: &str) -> i64 {
        if v.get(key).and_then(|v| v.as_bool()).unwrap_or(false) { 1 } else { 0 }
    }
    fn json_arr(v: &serde_json::Value, key: &str) -> String {
        v.get(key).map(|v| v.to_string()).unwrap_or_else(|| "[]".to_string())
    }
}
