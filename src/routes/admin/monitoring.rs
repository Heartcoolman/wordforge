use std::time::{Duration, Instant};

use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/health", get(system_health))
        .route("/database", get(database_stats))
        .route("/check-update", get(check_update))
}

// B62: System health monitoring
async fn system_health(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let db = state.store().raw_db();
    let size_on_disk = db.size_on_disk().unwrap_or(0);
    let uptime_secs = state.uptime_secs();
    let store_probe_ok = state.store().get_user_by_id("__health_check__").is_ok();
    let status = if store_probe_ok {
        "healthy"
    } else {
        "degraded"
    };

    Ok(ok(serde_json::json!({
        "status": status,
        "storeProbeOk": store_probe_ok,
        "dbSizeBytes": size_on_disk,
        "uptimeSecs": uptime_secs,
        "version": env!("CARGO_PKG_VERSION"),
    })))
}

async fn database_stats(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let db = state.store().raw_db();
    let trees: Vec<String> = db
        .tree_names()
        .iter()
        .map(|name| String::from_utf8_lossy(name.as_ref()).to_string())
        .collect();

    Ok(ok(serde_json::json!({
        "sizeOnDisk": db.size_on_disk().unwrap_or(0),
        "treeCount": trees.len(),
        "trees": trees,
    })))
}

const CACHE_TTL: Duration = Duration::from_secs(3600);

async fn check_update(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    {
        let cache = state.update_cache().read().await;
        if let Some((cached_at, ref data)) = *cache {
            if cached_at.elapsed() < CACHE_TTL {
                return Ok(ok(data.clone()));
            }
        }
    }

    let current_version = env!("CARGO_PKG_VERSION");

    match fetch_latest_release(current_version).await {
        Ok(data) => {
            *state.update_cache().write().await = Some((Instant::now(), data.clone()));
            Ok(ok(data))
        }
        Err(e) => {
            tracing::warn!("Failed to check for updates: {e}");
            let fallback = serde_json::json!({
                "currentVersion": current_version,
                "latestVersion": current_version,
                "hasUpdate": false,
                "releaseUrl": null,
                "releaseNotes": null,
            });
            *state.update_cache().write().await = Some((Instant::now(), fallback.clone()));
            Ok(ok(fallback))
        }
    }
}

async fn fetch_latest_release(
    current_version: &str,
) -> Result<serde_json::Value, Box<dyn std::error::Error + Send + Sync>> {
    let client = reqwest::Client::builder()
        .user_agent("wordforge-update-checker")
        .timeout(Duration::from_secs(10))
        .build()?;

    let resp = client
        .get("https://api.github.com/repos/Heartcoolman/wordforge/releases/latest")
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(format!("GitHub API returned {}", resp.status()).into());
    }

    let body: serde_json::Value = resp.json().await?;
    let tag_name = body["tag_name"].as_str().unwrap_or("");
    let latest_version = tag_name.trim_start_matches('v');
    let has_update = is_newer(latest_version, current_version);

    Ok(serde_json::json!({
        "currentVersion": current_version,
        "latestVersion": latest_version,
        "hasUpdate": has_update,
        "releaseUrl": body["html_url"].as_str(),
        "releaseNotes": body["body"].as_str(),
    }))
}

fn is_newer(latest: &str, current: &str) -> bool {
    let parse = |v: &str| -> Vec<u64> {
        v.split('.').filter_map(|s| s.parse().ok()).collect()
    };
    let l = parse(latest);
    let c = parse(current);
    for i in 0..3 {
        let lv = l.get(i).copied().unwrap_or(0);
        let cv = c.get(i).copied().unwrap_or(0);
        if lv > cv {
            return true;
        }
        if lv < cv {
            return false;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_versions() {
        assert!(!is_newer("0.1.3", "0.1.3"));
    }

    #[test]
    fn patch_increment() {
        assert!(is_newer("0.1.4", "0.1.3"));
        assert!(!is_newer("0.1.2", "0.1.3"));
    }

    #[test]
    fn minor_increment() {
        assert!(is_newer("0.2.0", "0.1.3"));
        assert!(!is_newer("0.0.9", "0.1.0"));
    }

    #[test]
    fn major_increment() {
        assert!(is_newer("1.0.0", "0.9.9"));
        assert!(!is_newer("0.9.9", "1.0.0"));
    }

    #[test]
    fn shorter_version_string() {
        assert!(!is_newer("0.1", "0.1.3"));
        assert!(is_newer("0.2", "0.1.3"));
    }

    #[test]
    fn prerelease_suffix_ignored() {
        // filter_map 会跳过无法解析的段，"3-beta" 解析失败变成空
        assert!(!is_newer("0.1.3-beta", "0.1.3"));
    }
}
