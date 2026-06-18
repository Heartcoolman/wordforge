//! v1.1-P2.2：迁移可逆性集成测试。
//!
//! 覆盖：
//!   1. up 全量 → revert_to(0) → schema 干净（migrations 独有的表 / 列消失，
//!      schema_version 表保留 + version=0）；
//!   2. revert_to(0) 后再次 up → schema_version 恢复到 N，所有 migration 独有表
//!      重新出现；
//!   3. 中间回退：revert_to(15) → schema 处于 m015 之后的状态 → 再 up → 回到 N；
//!   4. 重复多轮 up / down 不污染、不溢出。
//!
//! 注：down 仅供 dev / test 使用（详见 `src/store/migrate.rs` 顶部文档）。

use learning_backend::store::{migrate, Store};
use rusqlite::Connection;
use std::collections::{BTreeMap, BTreeSet};

/// 列出表是否存在（master 表 type='table'）。
fn table_exists(conn: &Connection, name: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name=?1",
            [name],
            |r| r.get(0),
        )
        .unwrap();
    n > 0
}

/// 列是否存在于指定表。
fn column_exists(conn: &Connection, table: &str, col: &str) -> bool {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({table})"))
        .unwrap();
    let names: Vec<String> = stmt
        .query_map([], |r| r.get::<_, String>(1))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    names.iter().any(|n| n == col)
}

/// 索引是否存在。
fn index_exists(conn: &Connection, name: &str) -> bool {
    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='index' AND name=?1",
            [name],
            |r| r.get(0),
        )
        .unwrap();
    n > 0
}

/// 当前迁移注册表总长度——通过先跑一遍 run 拿到 schema_version。
fn run_to_head(store: &Store) -> u32 {
    migrate::run(store).expect("up to head");
    migrate::get_current_version(store).expect("read version")
}

/// 单列签名:(name, type, notnull, dflt_value, pk)。
type ColSig = (String, String, i64, Option<String>, i64);
/// 全量 schema 快照:表名→列签名列表 + 索引名集合(排除 schema_version 与 sqlite 内部对象)。
fn dump_schema(conn: &Connection) -> (BTreeMap<String, Vec<ColSig>>, BTreeSet<String>) {
    let tables: Vec<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='table'
             AND name NOT LIKE 'sqlite_%' AND name <> 'schema_version' ORDER BY name",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    let mut cols = BTreeMap::new();
    for t in &tables {
        let sig: Vec<ColSig> = conn
            .prepare(&format!("PRAGMA table_info({t})"))
            .unwrap()
            .query_map([], |r| {
                // 规范化默认值：`DEFAULT NULL`(Some("NULL")) 与无默认(None) 在 SQLite 语义等价
                // （schema.rs DDL 写 DEFAULT NULL、迁移 up 的 ALTER ADD COLUMN 省略，纯表象不一致），
                // 归一为 None 后只比对真实差异(type/notnull/pk/非空默认)。
                let dflt: Option<String> = r.get(4)?;
                let dflt = match dflt.as_deref() {
                    Some(s) if s.eq_ignore_ascii_case("null") => None,
                    _ => dflt,
                };
                Ok((
                    r.get::<_, String>(1)?,
                    r.get::<_, String>(2)?,
                    r.get::<_, i64>(3)?,
                    dflt,
                    r.get::<_, i64>(5)?,
                ))
            })
            .unwrap()
            .filter_map(Result::ok)
            .collect();
        cols.insert(t.clone(), sig);
    }
    let indexes: BTreeSet<String> = conn
        .prepare(
            "SELECT name FROM sqlite_master WHERE type='index'
             AND name NOT LIKE 'sqlite_%'",
        )
        .unwrap()
        .query_map([], |r| r.get::<_, String>(0))
        .unwrap()
        .filter_map(Result::ok)
        .collect();
    (cols, indexes)
}

/// 插一条最小合法 users 行（仅 6 个 NOT NULL 无默认列），其余取默认。
fn seed_user(conn: &Connection, i: usize) {
    conn.execute(
        "INSERT INTO users (id, email, username, password_hash, created_at, updated_at)
         VALUES (?1, ?2, 'u', 'h', datetime('now'), datetime('now'))",
        rusqlite::params![format!("u{i}"), format!("u{i}@example.com")],
    )
    .unwrap();
}

fn users_count(store: &Store) -> i64 {
    store
        .connection()
        .unwrap()
        .query_row("SELECT count(*) FROM users", [], |r| r.get(0))
        .unwrap()
}

/// 真·任意版本回滚的承重不变式：对**每个**目标 schema 版本 k，
/// 「up→head → revert_to(k) → up→head」后的 schema 必须与原始 head 逐表逐列逐索引完全一致。
/// 这证明每个 down 是其 up 的干净逆——过度删除 / 错误删除 / down 或 up 中途失败都会破坏往返一致，
/// 从而被此 sweep 捕获。生产回滚正是「完整当前库 → revert_to(k)」这一方向。
#[test]
fn down_chain_round_trip_schema_identical_for_all_k() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);
    let baseline = dump_schema(&store.connection().unwrap());
    for k in 0..=head {
        migrate::revert_to(&store, k).unwrap_or_else(|e| panic!("revert_to({k}) 失败: {e}"));
        assert_eq!(
            migrate::get_current_version(&store).unwrap(),
            k,
            "revert_to({k}) 后版本号不符"
        );
        migrate::run(&store).unwrap_or_else(|e| panic!("从 {k} re-up 失败: {e}"));
        assert_eq!(
            migrate::get_current_version(&store).unwrap(),
            head,
            "从 {k} re-up 后未回到 head"
        );
        let after = dump_schema(&store.connection().unwrap());
        assert_eq!(after.0, baseline.0, "往返(k={k})后表/列结构漂移");
        assert_eq!(after.1, baseline.1, "往返(k={k})后索引集合漂移");
    }
}

/// 核心(基线)表数据无损：down 只删 migration 增量对象，绝不删 schema.rs 拥有的基线表的行
///（m001_down no-op）。seed users 后回退到若干代表性版本，行数必须不变；re-up 后仍不变。
#[test]
fn revert_preserves_baseline_table_rows() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);
    {
        let conn = store.connection().unwrap();
        for i in 0..5 {
            seed_user(&conn, i);
        }
    }
    assert_eq!(users_count(&store), 5);
    for k in [head - 1, head / 2, 1, 0] {
        migrate::revert_to(&store, k).unwrap();
        assert_eq!(users_count(&store), 5, "revert 到 {k} 后基线表 users 行数变化");
        migrate::run(&store).unwrap();
        assert_eq!(users_count(&store), 5, "从 {k} re-up 后 users 行数变化");
    }
}

/// `revert_db_copy` 文件级端到端：现役库降级产物达到目标 schema、保留基线表数据、现役库零接触、
/// 产物 sidecar 已清理。这是生产回滚降级引擎的真实路径。
#[test]
fn revert_db_copy_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let live_path = dir.path().join("learning.db");
    let live = Store::open(live_path.to_str().unwrap(), 5000, 2).unwrap();
    let head = run_to_head(&live);
    {
        let conn = live.connection().unwrap();
        seed_user(&conn, 1);
    }
    // pre-rollback 安全备份（干净单文件快照）
    let snapshot = dir.path().join("backup-pre-rollback.db");
    live.backup_to(&snapshot).unwrap();

    let target = head - 3;
    let out = dir.path().join("learning.db.rollback-src");
    migrate::revert_db_copy(&snapshot, &out, target).expect("revert_db_copy");

    // 产物 sidecar 已清理（在重新打开产物之前检查，否则重开会再建 WAL）
    let wal = std::path::PathBuf::from(format!("{}-wal", out.display()));
    assert!(!wal.exists(), "out-wal sidecar 应已清理");

    // 降级产物达到目标版本且基线数据保留
    let reverted = Store::open(out.to_str().unwrap(), 5000, 1).unwrap();
    assert_eq!(migrate::get_current_version(&reverted).unwrap(), target);
    assert_eq!(users_count(&reverted), 1, "降级产物应保留基线表数据");

    // 现役库未被触碰
    assert_eq!(
        migrate::get_current_version(&live).unwrap(),
        head,
        "revert_db_copy 不应改动现役库"
    );
    assert_eq!(users_count(&live), 1);
}

/// SCHEMA_VERSION 常量必须与迁移注册表长度一致（`--print-schema-version` 据此自报，不得漂移）。
#[test]
fn schema_version_const_matches_registry() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);
    assert_eq!(head, migrate::SCHEMA_VERSION, "SCHEMA_VERSION 与迁移注册表漂移");
    assert_eq!(migrate::SCHEMA_VERSION as usize, migrate::migration_count());
}

/// `revert_to(0)` 后：
///   - migrations 独有的表（schema.rs 没建过的）必须消失；
///   - schema.rs 共享的表仍可能存在（m001 是 no-op），但 m005/m013 等 ADD COLUMN
///     加的列必须不存在；
///   - schema_version 表本身保留，version 字段 = 0。
#[test]
fn revert_to_zero_clears_migration_only_tables_and_columns() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);
    assert!(head >= 20, "head 至少 20, got {head}");

    migrate::revert_to(&store, 0).expect("revert to 0");
    assert_eq!(migrate::get_current_version(&store).unwrap(), 0);

    let conn = store.connection().unwrap();

    // schema_version 必须保留（让下次 run 走增量路径）
    assert!(
        table_exists(&conn, "schema_version"),
        "schema_version 必须保留"
    );

    // migrations 独有、schema.rs 不建的表 —— 必须全部消失
    for t in [
        "gdpr_export_log",  // m015
        "update_audit_log", // m017
        "worker_last_run",  // m019
        "resource_packs",   // m020
        "resource_pack_versions",
        "resource_pack_active",
        "resource_pack_install_log",
    ] {
        assert!(!table_exists(&conn, t), "{t} 应该已被 revert 删除");
    }

    // ALTER ADD COLUMN 加的列必须消失
    assert!(
        !column_exists(&conn, "learning_records", "record_type"),
        "m005 record_type 应删除"
    );
    assert!(
        !column_exists(&conn, "learning_records", "self_rating"),
        "m013 self_rating 应删除"
    );
    for col in [
        "amas_auto_apply_enabled",
        "amas_auto_apply_max_per_day",
        "amas_auto_apply_min_confidence",
        "llm_advisor_max_cost_per_month_yuan",
    ] {
        assert!(
            !column_exists(&conn, "system_settings", col),
            "{col} 应已从 system_settings 删除"
        );
    }

    // 与 record_type 相关的索引也应消失
    assert!(!index_exists(&conn, "idx_learning_records_user_type_time"));
    assert!(!index_exists(&conn, "idx_learning_records_type_time"));

    // schema.rs 拥有的核心表仍存在（m001_down 是 no-op）
    assert!(table_exists(&conn, "users"));
    assert!(table_exists(&conn, "learning_records"));
}

/// revert → up 闭环：再次 up 应把所有 migration 独有表 / 列重新建出来，
/// schema_version 回到 head。
#[test]
fn up_after_revert_to_zero_restores_full_schema() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);

    migrate::revert_to(&store, 0).expect("revert");
    assert_eq!(migrate::get_current_version(&store).unwrap(), 0);

    migrate::run(&store).expect("up again");
    assert_eq!(migrate::get_current_version(&store).unwrap(), head);

    let conn = store.connection().unwrap();
    for t in [
        "gdpr_export_log",
        "update_audit_log",
        "worker_last_run",
        "resource_packs",
        "resource_pack_versions",
        "resource_pack_active",
        "resource_pack_install_log",
    ] {
        assert!(table_exists(&conn, t), "{t} 应在再次 up 后恢复");
    }
    assert!(column_exists(&conn, "learning_records", "record_type"));
    assert!(column_exists(&conn, "learning_records", "self_rating"));
    assert!(column_exists(
        &conn,
        "system_settings",
        "amas_auto_apply_enabled"
    ));
    assert!(column_exists(
        &conn,
        "system_settings",
        "llm_advisor_max_cost_per_month_yuan"
    ));
    assert!(index_exists(&conn, "idx_learning_records_user_type_time"));
    assert!(index_exists(&conn, "idx_learning_records_type_time"));
}

/// 中间回退：head → revert_to(15) → 此时 ≤ m015 的 schema 在场、> m015 的不在场 →
/// 再次 up → 回到 head。
#[test]
fn partial_revert_then_up_round_trip() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);
    assert!(head >= 20);

    migrate::revert_to(&store, 15).expect("revert to 15");
    assert_eq!(migrate::get_current_version(&store).unwrap(), 15);

    let conn = store.connection().unwrap();
    // ≤ m015 的副作用在场
    assert!(table_exists(&conn, "gdpr_export_log"), "m015 应仍在场");
    assert!(
        table_exists(&conn, "amas_tuning_suggestions"),
        "m010 应在场"
    );
    assert!(column_exists(&conn, "learning_records", "record_type"));
    // > m015 的副作用不在场
    assert!(!table_exists(&conn, "update_audit_log"), "m017 应已 revert");
    assert!(
        !column_exists(
            &conn,
            "system_settings",
            "llm_advisor_max_cost_per_month_yuan"
        ),
        "m016 列应已 revert"
    );
    assert!(!table_exists(&conn, "worker_last_run"), "m019 应已 revert");
    assert!(!table_exists(&conn, "resource_packs"), "m020 应已 revert");
    drop(conn);

    // up 到 head
    migrate::run(&store).expect("up to head");
    assert_eq!(migrate::get_current_version(&store).unwrap(), head);
    let conn = store.connection().unwrap();
    assert!(table_exists(&conn, "update_audit_log"));
    assert!(table_exists(&conn, "worker_last_run"));
    assert!(table_exists(&conn, "resource_packs"));
    assert!(column_exists(
        &conn,
        "system_settings",
        "llm_advisor_max_cost_per_month_yuan"
    ));
}

/// 多轮 up/down 循环：连续 3 轮 revert→up，每轮终态 schema_version 都应稳定回到 head。
#[test]
fn multiple_up_down_cycles_are_stable() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);
    for _ in 0..3 {
        migrate::revert_to(&store, 0).expect("revert");
        assert_eq!(migrate::get_current_version(&store).unwrap(), 0);
        migrate::run(&store).expect("up");
        assert_eq!(migrate::get_current_version(&store).unwrap(), head);
    }
    // 末轮的关键表仍正常
    let conn = store.connection().unwrap();
    assert!(table_exists(&conn, "resource_packs"));
    assert!(table_exists(&conn, "update_audit_log"));
    assert!(column_exists(&conn, "learning_records", "record_type"));
}

/// revert_to(target >= current) 是 no-op，不应报错也不应改 version。
#[test]
fn revert_to_noop_when_target_ge_current() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    let head = run_to_head(&store);

    migrate::revert_to(&store, head).expect("noop");
    assert_eq!(migrate::get_current_version(&store).unwrap(), head);

    migrate::revert_to(&store, head + 5).expect("noop above head");
    assert_eq!(migrate::get_current_version(&store).unwrap(), head);
}

/// 写入业务数据再回退：down 必然丢数据（DROP TABLE / DROP COLUMN），但 schema 必须
/// 干净——这是 down 文档化的代价。
#[test]
fn revert_drops_table_data_as_documented() {
    let store = Store::open(":memory:", 5000, 1).unwrap();
    run_to_head(&store);

    // 往 m017 拥有的表里写一条记录
    {
        let conn = store.connection().unwrap();
        conn.execute(
            "INSERT INTO update_audit_log
                (id, admin_id, from_version, to_version, channel, started_at, outcome)
             VALUES ('t1','admin','1.0.0','1.1.0','stable',datetime('now'),'in_progress')",
            [],
        )
        .unwrap();
    }

    migrate::revert_to(&store, 16).expect("revert past m017");
    let conn = store.connection().unwrap();
    assert!(
        !table_exists(&conn, "update_audit_log"),
        "m017 down 删表，数据自然丢失（已在文档说明）"
    );

    drop(conn);
    migrate::run(&store).expect("up again");
    let conn = store.connection().unwrap();
    assert!(table_exists(&conn, "update_audit_log"));
    let n: i64 = conn
        .query_row("SELECT count(*) FROM update_audit_log", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "重建后必然空表");
}
