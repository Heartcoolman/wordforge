//! B1:DB 备份离站外迁集成测试。
//! - file:// target:落本地目录可读回，retention prune 删旧。
//! - s3:// target:wiremock mock S3 endpoint，断言收到 PUT。
//! - 失败 target:断言落 system_alerts(source=backup_offsite)。

use std::sync::Arc;

use learning_backend::store::Store;
use learning_backend::workers::backup_offsite;
use serde_json::json;
use wiremock::matchers::method;
use wiremock::{Mock, MockServer, ResponseTemplate};

fn setup_store(name: &str) -> (tempfile::TempDir, Arc<Store>) {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join(name);
    let store = Arc::new(Store::open(path.to_str().unwrap(), 5000, 2).expect("open"));
    store.run_migrations().expect("migrations");
    (dir, store)
}

/// 写一个最小本地备份文件，返回其路径。
fn write_local_backup(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let p = dir.join(name);
    std::fs::write(&p, b"fake-sqlite-backup").unwrap();
    p
}

fn seed_backup_policy(store: &Store, targets: serde_json::Value) {
    let policy = json!({
        "fullCron": "0 3 * * *",
        "incrementalCron": "0 */6 * * *",
        "walArchive": false,
        "targets": targets,
    });
    store
        .upsert_settings_config("backup-policy", &policy)
        .unwrap();
}

#[tokio::test]
async fn file_target_copies_backup_and_prunes_old() {
    let (tmp, store) = setup_store("file_target.db");
    let local_dir = tmp.path().join("local");
    let remote_dir = tmp.path().join("offsite");
    let backup = write_local_backup(&local_dir, "backup-daily-20260602-030000.db");

    // 预置一个超期旧文件，验证 retention=1 天会删它
    std::fs::create_dir_all(&remote_dir).unwrap();
    let stale = remote_dir.join("backup-daily-20200101-000000.db");
    std::fs::write(&stale, b"old").unwrap();
    let old_time = filetime_two_days_ago();
    set_mtime(&stale, old_time);

    seed_backup_policy(
        &store,
        json!([{
            "name": "cold",
            "uri": format!("file://{}", remote_dir.display()),
            "retentionDays": 1u32,
        }]),
    );

    backup_offsite::run(&store, &backup).await;

    // 新备份已拷贝到离站目录
    let copied = remote_dir.join("backup-daily-20260602-030000.db");
    assert!(copied.exists(), "新备份应已拷贝到离站目录");
    assert_eq!(std::fs::read(&copied).unwrap(), b"fake-sqlite-backup");
    // 超期旧文件已被 prune
    assert!(!stale.exists(), "超期旧备份应被 prune 删除");

    // 无失败告警
    let since = "1970-01-01T00:00:00Z";
    let alerts = store.list_recent_system_alerts(since).unwrap();
    assert!(
        alerts.iter().all(|a| a.source != "backup_offsite"),
        "成功路径不应产生告警"
    );
}

#[tokio::test]
async fn s3_target_uploads_via_mock_endpoint() {
    let mock = MockServer::start().await;
    // S3 PUT object（object_store 校验响应需带 ETag header）
    Mock::given(method("PUT"))
        .respond_with(ResponseTemplate::new(200).insert_header("ETag", "\"abc123\""))
        .expect(1..)
        .mount(&mock)
        .await;
    Mock::given(method("GET"))
        .respond_with(ResponseTemplate::new(200).set_body_string(
            "<?xml version=\"1.0\"?><ListBucketResult><IsTruncated>false</IsTruncated></ListBucketResult>",
        ))
        .mount(&mock)
        .await;

    std::env::set_var("AWS_ACCESS_KEY_ID", "test");
    std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
    std::env::set_var("AWS_REGION", "us-east-1");
    std::env::set_var("AWS_ENDPOINT", mock.uri());
    std::env::set_var("AWS_ALLOW_HTTP", "true");

    let (tmp, store) = setup_store("s3_target.db");
    let backup = write_local_backup(&tmp.path().join("local"), "backup-daily-20260602-030000.db");

    seed_backup_policy(
        &store,
        json!([{
            "name": "primary",
            "uri": "s3://my-bucket/wf/backups",
            "retentionDays": 30u32,
        }]),
    );

    backup_offsite::run(&store, &backup).await;

    // mock 收到至少一个 PUT 即视为上传成功；同时不应有失败告警
    let alerts = store
        .list_recent_system_alerts("1970-01-01T00:00:00Z")
        .unwrap();
    assert!(
        alerts.iter().all(|a| a.source != "backup_offsite"),
        "S3 上传成功不应产生告警, got: {alerts:?}"
    );

    std::env::remove_var("AWS_ENDPOINT");
    std::env::remove_var("AWS_ALLOW_HTTP");
}

#[tokio::test]
async fn failed_target_records_system_alert() {
    let (tmp, store) = setup_store("fail_target.db");
    let backup = write_local_backup(&tmp.path().join("local"), "backup-daily-20260602-030000.db");

    // 不支持的 scheme → upload_one 返回 Err → 告警
    seed_backup_policy(
        &store,
        json!([{
            "name": "glacier-cold",
            "uri": "glacier://vault/wf",
            "retentionDays": 30u32,
        }]),
    );

    backup_offsite::run(&store, &backup).await;

    let alerts = store
        .list_recent_system_alerts("1970-01-01T00:00:00Z")
        .unwrap();
    let alert = alerts
        .iter()
        .find(|a| a.source == "backup_offsite")
        .expect("失败 target 应落 system_alerts");
    assert_eq!(alert.kind, "upload_failed");
    assert_eq!(alert.severity, "error");
    assert!(alert.title.contains("glacier-cold"));
}

#[tokio::test]
async fn no_policy_is_noop() {
    let (tmp, store) = setup_store("nopolicy.db");
    let backup = write_local_backup(&tmp.path().join("local"), "backup-daily-x.db");
    // 未配置 backup-policy section，应静默跳过，不 panic、不告警
    backup_offsite::run(&store, &backup).await;
    let alerts = store
        .list_recent_system_alerts("1970-01-01T00:00:00Z")
        .unwrap();
    assert!(alerts.iter().all(|a| a.source != "backup_offsite"));
}

// ───── filetime helpers（不引外部 crate，用 std + libc-free filetime via File::set_modified） ─────

fn filetime_two_days_ago() -> std::time::SystemTime {
    std::time::SystemTime::now() - std::time::Duration::from_secs(2 * 86400)
}

fn set_mtime(path: &std::path::Path, t: std::time::SystemTime) {
    // std 1.75+ File::set_modified
    let f = std::fs::OpenOptions::new().write(true).open(path).unwrap();
    f.set_modified(t).unwrap();
}
