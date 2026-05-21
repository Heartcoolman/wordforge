/// M0-C3 验证：SseEvent 变体数必须与 docs/api-endpoints.md SSE 表行数一致。
///
/// 维护策略：
/// - 每新增一个 SseEvent 变体时，在 DOCUMENTED_EVENTS 里追加对应的 `type` 值；
/// - 每新增一个文档表行时，在此列表追加，并在 state.rs 添加对应变体；
/// - 测试会同时校验所有已记录变体的 JSON `type` 字段序列化正确，确保 wire 与文档一致。
use learning_backend::state::SseEvent;

/// docs/api-endpoints.md §14 SSE 表中记录的全部事件名（`event:` 字段），
/// 不含 `amas_state`（由独立通路发出，不属于 SseEvent enum）
/// 和 `update_available`（由 broadcast_update 发出，也不属于 SseEvent enum）。
const DOCUMENTED_EVENTS: &[&str] = &[
    "maintenance",
    "telemetry_request",
    "banned",
    "unbanned",
    "data_corrupted",
    "new_llm_suggestion",
    "release_available",
    "update_progress",
    "probe_request",
    "probe_confirm",
    "incident",
];

/// SseEvent 变体的穷举列表，用于序列化验证。
/// 当 state.rs 新增变体时，必须在此同步追加。
fn all_sse_event_samples() -> Vec<SseEvent> {
    use learning_backend::services::updater::Channel;
    vec![
        SseEvent::Maintenance { active: true },
        SseEvent::TelemetryRequest {
            request_id: "req-id".to_string(),
        },
        SseEvent::Banned,
        SseEvent::Unbanned,
        SseEvent::DataCorrupted,
        SseEvent::NewLlmSuggestion { suggestion_id: 1 },
        SseEvent::ReleaseAvailable {
            latest_tag: "v0.5.6".to_string(),
            channel: Channel::Stable,
        },
        SseEvent::UpdateProgress {
            phase: "downloading".to_string(),
            percent: 35,
        },
        SseEvent::ProbeRequest {
            request_id: "req".to_string(),
            batch_id: "batch".to_string(),
            script_b64: "c2NyaXB0".to_string(),
            timeout_ms: 3000,
            ctx_version: 1,
        },
        SseEvent::ProbeConfirm {
            request_id: "req".to_string(),
            confirm_token: "tok".to_string(),
        },
        SseEvent::Incident {
            error_rate: 0.025,
            window_secs: 300,
        },
    ]
}

#[test]
fn sse_event_variant_count_matches_docs() {
    let samples = all_sse_event_samples();
    // 变体样本数必须等于文档表记录数
    assert_eq!(
        samples.len(),
        DOCUMENTED_EVENTS.len(),
        "SseEvent 样本数 ({}) 与文档表行数 ({}) 不一致；请同步更新 state.rs 和 docs/api-endpoints.md",
        samples.len(),
        DOCUMENTED_EVENTS.len(),
    );
}

#[test]
fn sse_event_type_fields_match_docs() {
    let samples = all_sse_event_samples();
    for (event, expected_type) in samples.iter().zip(DOCUMENTED_EVENTS.iter()) {
        let json = serde_json::to_value(event).expect("SseEvent serialize");
        let actual_type = json["type"].as_str().expect("type field missing");
        assert_eq!(
            actual_type, *expected_type,
            "SseEvent 序列化 type 字段 {:?} 与文档记录 {:?} 不一致",
            actual_type, expected_type,
        );
    }
}
