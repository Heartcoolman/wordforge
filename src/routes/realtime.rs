use std::convert::Infallible;
use std::time::Duration;

use axum::response::sse::{Event, KeepAlive, Sse};
use axum::routing::get;
use axum::{extract::State, Router};
use futures::Stream;

use crate::auth::AuthUser;
use crate::state::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/events", get(sse_handler))
}

pub async fn sse_handler(
    auth: AuthUser,
    State(state): State<AppState>,
) -> Sse<impl Stream<Item = Result<Event, Infallible>>> {
    let mut shutdown_rx = state.shutdown_rx();
    let user_id = auth.user_id.clone();

    let stream = async_stream::stream! {
        let mut interval = tokio::time::interval(Duration::from_secs(5));
        let mut last_event_count: u64 = 0;

        // Get initial state
        if let Ok(user_state) = state.amas().get_user_state(&user_id) {
            last_event_count = user_state.total_event_count;
        }

        loop {
            tokio::select! {
                _ = interval.tick() => {
                    // B77: Check for mastery events
                    // B78: Check for AMAS state changes
                    if let Ok(user_state) = state.amas().get_user_state(&user_id) {
                        if user_state.total_event_count > last_event_count {
                            // State has changed, emit event
                            let event_data = serde_json::json!({
                                "type": "state_change",
                                "attention": user_state.attention,
                                "fatigue": user_state.fatigue,
                                "motivation": user_state.motivation,
                                "totalEvents": user_state.total_event_count,
                            });

                            if let Ok(json) = serde_json::to_string(&event_data) {
                                yield Ok(Event::default()
                                    .event("amas_state")
                                    .data(json));
                            }

                            last_event_count = user_state.total_event_count;
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    break;
                }
            }
        }
    };

    Sse::new(stream).keep_alive(
        KeepAlive::new()
            .interval(Duration::from_secs(15))
            .text("keepalive"),
    )
}
