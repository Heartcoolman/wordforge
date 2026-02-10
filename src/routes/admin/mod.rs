pub mod amas;
pub mod analytics;
pub mod auth;
pub mod broadcast;
pub mod monitoring;
pub mod settings;

use axum::extract::{Path, State};
use axum::routing::{get, post};
use axum::Router;
use serde::Serialize;

use crate::auth::AdminAuthUser;
use crate::response::{ok, AppError};
use crate::state::AppState;
use crate::store::operations::users::User;

/// Safe admin view of a user (excludes password_hash).
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct AdminUserView {
    id: String,
    email: String,
    username: String,
    is_banned: bool,
    created_at: chrono::DateTime<chrono::Utc>,
    updated_at: chrono::DateTime<chrono::Utc>,
}

impl From<&User> for AdminUserView {
    fn from(u: &User) -> Self {
        Self {
            id: u.id.clone(),
            email: u.email.clone(),
            username: u.username.clone(),
            is_banned: u.is_banned,
            created_at: u.created_at,
            updated_at: u.updated_at,
        }
    }
}

pub fn router() -> Router<AppState> {
    Router::new()
        .nest("/auth", auth::router())
        .nest("/analytics", analytics::router())
        .nest("/monitoring", monitoring::router())
        .nest("/broadcast", broadcast::router())
        .nest("/settings", settings::router())
        .route("/users", get(list_users))
        .route("/users/:id/ban", post(ban_user))
        .route("/users/:id/unban", post(unban_user))
        .route("/stats", get(admin_stats))
}

async fn list_users(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let users = state.store().list_users(200, 0)?;
    let safe_users: Vec<AdminUserView> = users.iter().map(AdminUserView::from).collect();
    Ok(ok(safe_users))
}

async fn ban_user(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state.store().ban_user(&id)?;
    Ok(ok(serde_json::json!({"banned": true, "userId": id})))
}

async fn unban_user(
    _admin: AdminAuthUser,
    Path(id): Path<String>,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    state.store().unban_user(&id)?;
    Ok(ok(serde_json::json!({"banned": false, "userId": id})))
}

async fn admin_stats(
    _admin: AdminAuthUser,
    State(state): State<AppState>,
) -> Result<impl axum::response::IntoResponse, AppError> {
    let users_list = state.store().list_users(10_000, 0)?;
    let user_count = users_list.len();
    let word_count = state.store().count_words()?;

    let mut record_count = 0usize;
    for user in &users_list {
        record_count += state.store().count_user_records(&user.id)?;
    }

    Ok(ok(serde_json::json!({
        "users": user_count,
        "words": word_count,
        "records": record_count,
    })))
}
