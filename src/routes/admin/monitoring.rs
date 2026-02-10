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

    Ok(ok(serde_json::json!({
        "status": "healthy",
        "dbSizeBytes": size_on_disk,
        "uptime": "unknown",
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
        "trees": db.tree_names().iter().map(|n| String::from_utf8_lossy(n).to_string()).collect::<Vec<_>>(),
    })))
}
