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

    Ok(ok(serde_json::json!({
        "sizeOnDisk": db.size_on_disk().unwrap_or(0),
        "treeCount": db.tree_names().len(),
    })))
}
