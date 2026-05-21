pub mod batch;
pub mod single;
pub mod stats;

pub(crate) use single::{
    capture_user_state_snapshot, restore_user_state_snapshot, CreateRecordRequest,
};
pub(crate) use batch::process_batch_record;

use axum::Router;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(single::router())
        .merge(batch::router())
        .merge(stats::router())
}
