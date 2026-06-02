pub mod batch;
pub mod single;
pub mod stats;

pub(crate) use batch::process_batch_record;
pub(crate) use single::{
    capture_user_state_snapshot, restore_user_state_snapshot, CreateRecordRequest,
};

use crate::state::AppState;
use axum::Router;

pub fn router() -> Router<AppState> {
    Router::new()
        .merge(single::router())
        .merge(batch::router())
        .merge(stats::router())
}
