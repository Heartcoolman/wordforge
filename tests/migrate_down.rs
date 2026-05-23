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
        "gdpr_export_log",       // m015
        "update_audit_log",      // m017
        "worker_last_run",       // m019
        "resource_packs",        // m020
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
    assert!(table_exists(&conn, "amas_tuning_suggestions"), "m010 应在场");
    assert!(column_exists(&conn, "learning_records", "record_type"));
    // > m015 的副作用不在场
    assert!(!table_exists(&conn, "update_audit_log"), "m017 应已 revert");
    assert!(
        !column_exists(&conn, "system_settings", "llm_advisor_max_cost_per_month_yuan"),
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
