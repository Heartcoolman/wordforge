use axum::extract::State;
use axum::routing::get;
use axum::Router;

use crate::response::ok;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(get_status))
}

async fn get_status(State(state): State<AppState>) -> impl axum::response::IntoResponse {
    ok(serde_json::json!({
        "maintenanceMode": state.is_maintenance(),
        "version": env!("CARGO_PKG_VERSION"),
    }))
}
