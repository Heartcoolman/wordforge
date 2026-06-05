//! Updater 服务的集成测试：用 wiremock 假装 GitHub API + 资产 URL，覆盖
//! ETag 短路、sha256 校验、downgrade 守门、限流、apply 早退（在 backup callback
//! 抛错以避免真去 fork-exec）。

use std::path::Path;
use std::sync::Arc;

use learning_backend::config::UpdateCheckConfig;
use learning_backend::services::updater::{
    ApplyContext, Channel, ProgressSink, Updater, UpdaterError,
};
use sha2::{Digest, Sha256};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn build_tarball() -> Vec<u8> {
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tar::{Builder, Header};

    let mut tar_buf = Vec::new();
    {
        let mut builder = Builder::new(&mut tar_buf);

        // root dir
        let mut dir = Header::new_gnu();
        dir.set_path("wordforge-linux-x86_64/").unwrap();
        dir.set_entry_type(tar::EntryType::Directory);
        dir.set_mode(0o755);
        dir.set_size(0);
        dir.set_cksum();
        builder.append(&dir, std::io::empty()).unwrap();

        // binary
        let mut bin = Header::new_gnu();
        bin.set_path("wordforge-linux-x86_64/wordforge").unwrap();
        bin.set_mode(0o755);
        let bin_body: &[u8] = b"#!/bin/sh\necho fake-wordforge\n";
        bin.set_size(bin_body.len() as u64);
        bin.set_cksum();
        builder.append(&bin, bin_body).unwrap();

        // static dir
        let mut st_dir = Header::new_gnu();
        st_dir.set_path("wordforge-linux-x86_64/static/").unwrap();
        st_dir.set_entry_type(tar::EntryType::Directory);
        st_dir.set_mode(0o755);
        st_dir.set_size(0);
        st_dir.set_cksum();
        builder.append(&st_dir, std::io::empty()).unwrap();

        // static/index.html
        let mut idx = Header::new_gnu();
        idx.set_path("wordforge-linux-x86_64/static/index.html")
            .unwrap();
        idx.set_mode(0o644);
        let idx_body: &[u8] = b"<html></html>";
        idx.set_size(idx_body.len() as u64);
        idx.set_cksum();
        builder.append(&idx, idx_body).unwrap();

        builder.finish().unwrap();
    }

    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&tar_buf).unwrap();
    gz.finish().unwrap()
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn build_release_json(server_base: &str, tag: &str, size: u64) -> serde_json::Value {
    let arch = "x86_64";
    serde_json::json!({
        "tag_name": tag,
        "name": tag,
        "html_url": format!("https://example.com/r/{tag}"),
        "body": "## Test release",
        "published_at": "2026-05-17T16:00:00Z",
        "assets": [
            {
                "name": format!("wordforge-linux-{arch}.tar.gz"),
                "browser_download_url": format!("{server_base}/dl/wordforge-linux-{arch}.tar.gz"),
                "size": size,
            },
            {
                "name": format!("wordforge-linux-{arch}.tar.gz.sha256"),
                "browser_download_url": format!("{server_base}/dl/wordforge-linux-{arch}.tar.gz.sha256"),
                "size": 64,
            }
        ]
    })
}

fn build_cfg(install_dir: &Path, api_url: &str) -> UpdateCheckConfig {
    UpdateCheckConfig {
        api_url: api_url.to_string(),
        cache_ttl_secs: 0, // 强制每次走网络以方便测试
        worker_enabled: false,
        worker_interval_secs: 3600,
        github_token: None,
        allow_downgrade: false,
        install_dir: Some(install_dir.to_path_buf()),
        max_tarball_bytes: 50 * 1024 * 1024,
        download_mirror_prefix: None,
    }
}

fn noop_sink() -> ProgressSink {
    Arc::new(|_| {})
}

fn noop_ctx(channel: Channel, tag: &str) -> ApplyContext {
    ApplyContext {
        channel,
        target_tag: tag.to_string(),
        health_url: String::new(),
        on_rollback: Box::new(|_| {}),
        on_maintenance: Box::new(|_| {}),
        task_id: "test-task-id".to_string(),
        allow_downgrade: false,
    }
}

fn skip_unless_supported_arch() -> bool {
    !matches!(std::env::consts::ARCH, "x86_64" | "aarch64")
}

#[tokio::test]
async fn check_latest_caches_and_returns_status() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let release = build_release_json(&server.uri(), "v9.9.9", tarball.len() as u64);
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"etag-1\"")
                .set_body_json(release),
        )
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();

    let status = updater.check_latest().await.expect("check ok");
    assert_eq!(status.current_version, "v0.4.2");
    assert_eq!(
        status.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v9.9.9")
    );
    assert!(status.beta.as_ref().map(|c| c.has_update).unwrap_or(false));
    assert!(status.beta.as_ref().map(|c| c.can_apply).unwrap_or(false));

    // Snapshot 返回相同视图
    let snap = updater.snapshot().await;
    assert_eq!(
        snap.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v9.9.9")
    );
}

#[tokio::test]
async fn etag_304_short_circuits_without_overwriting_cache() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let release = build_release_json(&server.uri(), "v9.9.9", tarball.len() as u64);

    // 第一次请求：200 + ETag
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("etag", "\"etag-2\"")
                .set_body_json(release.clone()),
        )
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // 后续请求：304
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .and(header("if-none-match", "\"etag-2\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();

    let _ = updater.check_latest().await.unwrap();
    let second = updater.check_latest().await.unwrap();
    // 304 路径仍保留 latest
    assert_eq!(
        second.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v9.9.9")
    );
}

#[tokio::test]
async fn apply_rejects_sha256_mismatch() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let release = build_release_json(&server.uri(), "v9.9.9", tarball.len() as u64);
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dl/wordforge-linux-x86_64.tar.gz.sha256"))
        .respond_with(
            ResponseTemplate::new(200).set_body_string("deadbeef".repeat(8)), // 64 hex chars but won't match
        )
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dl/wordforge-linux-x86_64.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(tarball.clone()))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();
    let _ = updater.check_latest().await.unwrap();
    let backup = |_dst: &Path| Ok(());
    let err = updater
        .apply(noop_ctx(Channel::Beta, "v9.9.9"), backup, noop_sink())
        .await
        .unwrap_err();
    assert!(
        matches!(err, UpdaterError::Sha256Mismatch { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn apply_rejects_downgrade_by_default() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let release = build_release_json(&server.uri(), "v0.1.0", tarball.len() as u64);
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v9.9.9").unwrap();
    let _ = updater.check_latest().await.unwrap();
    let backup = |_dst: &Path| Ok(());
    let err = updater
        .apply(noop_ctx(Channel::Beta, "v0.1.0"), backup, noop_sink())
        .await
        .unwrap_err();
    assert!(
        matches!(err, UpdaterError::DowngradeRefused { .. }),
        "got {err:?}"
    );
}

#[tokio::test]
async fn apply_rejects_invalid_target_tag() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let release = build_release_json(&server.uri(), "v9.9.9", 100);
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();
    let _ = updater.check_latest().await.unwrap();
    let backup = |_dst: &Path| Ok(());
    let err = updater
        .apply(noop_ctx(Channel::Beta, "v0.0.999"), backup, noop_sink())
        .await
        .unwrap_err();
    assert!(matches!(err, UpdaterError::InvalidTarget(_)), "got {err:?}");
}

#[tokio::test]
async fn apply_runs_until_backup_callback_when_sha_matches() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let sha_hex = sha256_hex(&tarball);
    let release = build_release_json(&server.uri(), "v9.9.9", tarball.len() as u64);
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dl/wordforge-linux-x86_64.tar.gz.sha256"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha_hex))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dl/wordforge-linux-x86_64.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(tarball.clone()))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();
    let _ = updater.check_latest().await.unwrap();

    // 用 backup callback 抛错来阻断后续的 fork-exec，同时验证它确实被调用
    let called = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let called_cb = called.clone();
    let backup = move |_dst: &Path| {
        called_cb.store(true, std::sync::atomic::Ordering::SeqCst);
        Err(UpdaterError::Config("intentional test abort".into()))
    };

    let err = updater
        .apply(noop_ctx(Channel::Beta, "v9.9.9"), backup, noop_sink())
        .await
        .unwrap_err();
    assert!(matches!(err, UpdaterError::Config(_)));
    assert!(called.load(std::sync::atomic::Ordering::SeqCst));
    // tarball 应已校验通过，staging 应被建出
    assert!(tmp.path().join(".update-staging").exists());
}

// ────── Codex review follow-ups ──────

/// Codex P1: 恶意 tarball 用 symlink 跳出 dst。即便 entry path 全部"看起来合法"，
/// EntryType::Symlink 也必须直接拒掉。
#[tokio::test]
async fn apply_rejects_tarball_with_symlink_entries() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    // 手工组装带 symlink 的 tarball
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;
    use tar::{Builder, Header};

    let mut tar_buf = Vec::new();
    {
        let mut b = Builder::new(&mut tar_buf);

        // 顶层目录
        let mut d = Header::new_gnu();
        d.set_path("wordforge-linux-x86_64/").unwrap();
        d.set_entry_type(tar::EntryType::Directory);
        d.set_mode(0o755);
        d.set_size(0);
        d.set_cksum();
        b.append(&d, std::io::empty()).unwrap();

        // 恶意 symlink：wordforge-linux-x86_64/wordforge → /tmp/wf-evil-target
        let mut s = Header::new_gnu();
        s.set_path("wordforge-linux-x86_64/wordforge").unwrap();
        s.set_entry_type(tar::EntryType::Symlink);
        s.set_link_name("/tmp/wf-evil-target").unwrap();
        s.set_mode(0o777);
        s.set_size(0);
        s.set_cksum();
        b.append(&s, std::io::empty()).unwrap();

        b.finish().unwrap();
    }
    let mut gz = GzEncoder::new(Vec::new(), Compression::default());
    gz.write_all(&tar_buf).unwrap();
    let tarball = gz.finish().unwrap();
    let sha_hex = sha256_hex(&tarball);

    let server = MockServer::start().await;
    let release = build_release_json(&server.uri(), "v9.9.9", tarball.len() as u64);
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dl/wordforge-linux-x86_64.tar.gz.sha256"))
        .respond_with(ResponseTemplate::new(200).set_body_string(sha_hex))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/dl/wordforge-linux-x86_64.tar.gz"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(tarball.clone()))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();
    let _ = updater.check_latest().await.unwrap();
    let backup = |_dst: &Path| Ok(());
    let err = updater
        .apply(noop_ctx(Channel::Beta, "v9.9.9"), backup, noop_sink())
        .await
        .unwrap_err();
    assert!(matches!(err, UpdaterError::UnsafePath(_)), "got {err:?}");
    // /tmp/wf-evil-target 不应被任何东西创建出来
    assert!(!std::path::Path::new("/tmp/wf-evil-target").exists());
}

/// Codex P2: `force_check_latest` 必须无视 TTL 真打 GitHub。
#[tokio::test]
async fn force_check_bypasses_ttl_cache() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let release_v1 = build_release_json(&server.uri(), "v1.0.0", tarball.len() as u64);
    let release_v2 = build_release_json(&server.uri(), "v2.0.0", tarball.len() as u64);

    // 第一次：返 v1
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_v1))
        .up_to_n_times(1)
        .mount(&server)
        .await;
    // 后续：返 v2
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(release_v2))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    // 用一个很长的 TTL 保证非 force 会命中缓存
    let mut cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    cfg.cache_ttl_secs = 3600;
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();

    let first = updater.check_latest().await.unwrap();
    assert_eq!(
        first.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v1.0.0")
    );

    // 普通 check_latest 仍命中 TTL，返回 v1
    let cached = updater.check_latest().await.unwrap();
    assert_eq!(
        cached.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v1.0.0")
    );

    // force_check_latest 必须真去打，拿到 v2
    let fresh = updater.force_check_latest().await.unwrap();
    assert_eq!(
        fresh.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v2.0.0")
    );
}

/// Codex P3: tar.gz / sha256 资产任一缺失，apply 必须早抛 NoAsset。
#[tokio::test]
async fn apply_rejects_release_missing_assets() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    // release 但 assets 数组里只有非匹配项
    let server = MockServer::start().await;
    let mispackaged = serde_json::json!({
        "tag_name": "v9.9.9",
        "html_url": "https://example.com/r/v9.9.9",
        "body": "## broken release",
        "published_at": "2026-05-17T16:00:00Z",
        "assets": [
            {
                "name": "wordforge-linux-mips.tar.gz",
                "browser_download_url": "https://example.com/wrong-arch.tar.gz",
                "size": 100,
            }
        ],
    });
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(200).set_body_json(mispackaged))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();

    let status = updater.check_latest().await.unwrap();
    assert!(
        !status.beta.as_ref().map(|c| c.can_apply).unwrap_or(false),
        "缓存了 release 但 canApply 应为 false"
    );
    assert_eq!(
        status.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v9.9.9")
    );

    let backup = |_dst: &Path| Ok(());
    let err = updater
        .apply(noop_ctx(Channel::Beta, "v9.9.9"), backup, noop_sink())
        .await
        .unwrap_err();
    assert!(matches!(err, UpdaterError::NoAsset { .. }), "got {err:?}");
}

/// Codex P1 (2nd pass): 进程重启后 .update_etag 还原但 cache.latest=None，
/// 此时若发条件请求 + 拿到 304 会永远卡住。修复确保第一次发的是无条件请求。
#[tokio::test]
async fn etag_loaded_from_disk_without_payload_triggers_unconditional_request() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    let tarball = build_tarball();
    let release = build_release_json(&server.uri(), "v9.9.9", tarball.len() as u64);

    // 只接受**没有** If-None-Match 头的请求 → 返 200 + body
    use wiremock::matchers::header_exists;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .and(wiremock::matchers::any())
        .respond_with(ResponseTemplate::new(200).set_body_json(release))
        .mount(&server)
        .await;
    // 如果错误地发了 If-None-Match，wiremock 没有匹配规则 → 返 404；
    // 但更稳的是用一个互斥 mock：If-None-Match 存在 → 500，让测试直接失败
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .and(header_exists("If-None-Match"))
        .respond_with(ResponseTemplate::new(500).set_body_string("ETAG SHOULD NOT BE SENT"))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    // 预先把一个 etag 写到磁盘，模拟"上次 run 写过 etag"
    std::fs::write(tmp.path().join(".update_etag"), "\"stale-etag-from-disk\"").unwrap();

    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();

    // 关键：第一次 check 必须无视磁盘上的 etag（因为内存里没 latest payload）
    let status = updater.check_latest().await.expect("must not 500");
    assert_eq!(
        status.beta.as_ref().map(|c| c.latest_version.as_str()),
        Some("v9.9.9")
    );
    assert!(status.beta.as_ref().map(|c| c.has_update).unwrap_or(false));
}

#[tokio::test]
async fn rate_limit_propagates_as_dedicated_error() {
    if skip_unless_supported_arch() || std::env::consts::OS != "linux" {
        return;
    }
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/repos/o/r/releases/latest"))
        .respond_with(ResponseTemplate::new(403).set_body_string("rate limit exceeded"))
        .mount(&server)
        .await;

    let tmp = tempfile::tempdir().unwrap();
    let cfg = build_cfg(
        tmp.path(),
        &format!("{}/repos/o/r/releases/latest", server.uri()),
    );
    let updater = Updater::new(&cfg, "v0.4.2").unwrap();
    let err = updater.check_latest().await.unwrap_err();
    assert!(matches!(err, UpdaterError::RateLimited), "got {err:?}");
}
