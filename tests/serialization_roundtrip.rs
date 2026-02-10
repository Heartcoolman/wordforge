use learning_backend::amas::types::{StrategyParams, UserState};

#[test]
fn pt_serialization_roundtrip() {
    let strategy = StrategyParams::default();
    let encoded = serde_json::to_string(&strategy).expect("serialize strategy");
    let decoded: StrategyParams = serde_json::from_str(&encoded).expect("deserialize strategy");
    assert_eq!(decoded.batch_size, strategy.batch_size);

    let state = UserState::default();
    let encoded_state = serde_json::to_string(&state).expect("serialize state");
    let decoded_state: UserState = serde_json::from_str(&encoded_state).expect("deserialize state");
    assert_eq!(decoded_state.total_event_count, state.total_event_count);
}
