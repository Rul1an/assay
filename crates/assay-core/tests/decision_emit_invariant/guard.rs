use crate::fixtures::TestEmitter;
use assay_core::mcp::decision::{Decision, DecisionEmitterGuard};
use std::sync::Arc;

#[test]
fn test_guard_drop_emits_on_early_return() {
    let emitter = Arc::new(TestEmitter::new());

    fn simulate_early_return(emitter: Arc<TestEmitter>) {
        let _guard = DecisionEmitterGuard::new(
            emitter,
            "assay://test".to_string(),
            "tc_001".to_string(),
            "test_tool".to_string(),
        );
    }

    simulate_early_return(emitter.clone());

    assert_eq!(emitter.event_count(), 1, "Guard must emit on drop");
    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Error);
}

#[test]
fn test_guard_emits_on_panic() {
    let emitter = Arc::new(TestEmitter::new());
    let emitter_clone = emitter.clone();

    let result = std::panic::catch_unwind(move || {
        let _guard = DecisionEmitterGuard::new(
            emitter_clone,
            "assay://test".to_string(),
            "tc_panic".to_string(),
            "panic_tool".to_string(),
        );
        panic!("Simulated panic");
    });

    assert!(result.is_err(), "Should have panicked");
    assert_eq!(emitter.event_count(), 1, "Guard must emit even on panic");

    let event = emitter.last_event().expect("Should have event");
    assert_eq!(event.data.decision, Decision::Error);
    assert_eq!(event.data.tool_call_id, "tc_panic");
}
