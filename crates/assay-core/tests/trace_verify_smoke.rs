use assay_core::model::{EvalConfig, Expected, Settings, TestCase, TestInput};
use assay_core::trace::verify::verify_coverage;
use std::io::Write;
use tempfile::NamedTempFile;

fn make_config(tests: Vec<TestCase>) -> EvalConfig {
    EvalConfig {
        version: 2,
        suite: "verify_smoke_suite".to_string(),
        model: "test_model".to_string(),
        settings: Settings::default(),
        thresholds: Default::default(),
        otel: Default::default(),
        tests,
    }
}

fn make_test_case(id: &str, prompt: &str) -> TestCase {
    TestCase {
        id: id.to_string(),
        input: TestInput {
            prompt: prompt.to_string(),
            context: None,
        },
        expected: Expected::MustContain {
            must_contain: vec![],
        },
        assertions: None,
        on_error: None,
        tags: vec![],
        metadata: None,
    }
}

fn create_trace_file(prompts: &[(&str, &str)]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("create temp trace file");
    for (ep_id, prompt) in prompts {
        let line = serde_json::json!({
            "type": "episode_start",
            "episode_id": ep_id,
            "timestamp": 1700000000000u64,
            "input": {
                "prompt": prompt
            },
            "meta": {}
        });
        writeln!(file, "{}", serde_json::to_string(&line).unwrap()).unwrap();
    }
    file.flush().unwrap();
    file
}

#[test]
fn test_verify_coverage_exact_match_success() {
    let cfg = make_config(vec![
        make_test_case("test-1", "What is the capital of France?"),
        make_test_case("test-2", "Compute 2 + 2"),
    ]);

    let trace = create_trace_file(&[
        ("ep-1", "What is the capital of France?"),
        ("ep-2", "Compute 2 + 2"),
    ]);

    let res = verify_coverage(trace.path(), &cfg);
    assert!(
        res.is_ok(),
        "Exact matches must pass trace verification: {:?}",
        res.err()
    );
}

#[test]
fn test_verify_coverage_genuinely_missing_diagnostic() {
    let cfg = make_config(vec![
        make_test_case("test-present", "Known prompt"),
        make_test_case("test-missing", "Prompt that is not in the trace"),
    ]);

    let trace = create_trace_file(&[("ep-1", "Known prompt")]);

    let err = verify_coverage(trace.path(), &cfg).unwrap_err();
    let msg = format!("{:#}", err);

    assert!(
        msg.contains("missing matching prompt in trace"),
        "Diagnostic must report missing prompt: {}",
        msg
    );
    assert!(
        msg.contains("test-missing"),
        "Diagnostic must name missing test id: {}",
        msg
    );
    assert!(
        !msg.contains("matches stage-local truncation shape")
            && !msg.contains("matches the truncation shape"),
        "Genuinely missing test must not be described as matching truncation shape: {}",
        msg
    );
}

#[test]
fn test_verify_coverage_truncation_shape_diagnostic() {
    // Over-4,096-byte prompt with multibyte characters
    let chunk = "🦀 prompt with multibyte 日本語 and €uro: ";
    let repeat_count = (5000 / chunk.len()) + 1;
    let long_prompt = chunk.repeat(repeat_count);
    assert!(long_prompt.len() > 4096);
    assert!(!long_prompt.is_ascii());

    let cfg = make_config(vec![make_test_case("test-over-ceiling", &long_prompt)]);

    // Trace contains the prompt; when parsed by StreamUpgrader, it is stage-locally truncated
    let trace = create_trace_file(&[("ep-long", &long_prompt)]);

    let err = verify_coverage(trace.path(), &cfg).expect_err(
        "Truncation-shape match MUST still return error/non-success; never turn into exact coverage",
    );
    let msg = format!("{:#}", err);

    assert!(
        msg.contains("matches stage-local truncation shape")
            || msg.contains("matches the truncation shape"),
        "Diagnostic must report truncation shape match: {}",
        msg
    );
    assert!(
        msg.contains("exact prompt coverage cannot be established"),
        "Diagnostic must state uncertainty (exact prompt coverage cannot be established): {}",
        msg
    );
    assert!(
        msg.contains("test-over-ceiling"),
        "Diagnostic must name the affected test: {}",
        msg
    );
    assert!(
        !msg.contains("missing matching prompt in trace"),
        "Truncation-shape test must not be labeled as genuinely missing: {}",
        msg
    );
}

#[test]
fn test_verify_coverage_distinct_long_prompts_sharing_prefix() {
    // Two distinct prompts that share the retained prefix but differ after 4082 bytes
    let chunk = "🦀 prompt payload with multibyte 日本語: ";
    let repeat_count = (4100 / chunk.len()) + 1;
    let common_prefix = chunk.repeat(repeat_count);

    let prompt_a = format!("{}AAAA_DISTINCT_TAIL_ONE", common_prefix);
    let prompt_b = format!("{}BBBB_DISTINCT_TAIL_TWO", common_prefix);
    assert!(prompt_a.len() > 4096);
    assert!(prompt_b.len() > 4096);
    assert_ne!(prompt_a, prompt_b);

    // Both prompts produce the exact same stage-local truncation shape
    let cfg = make_config(vec![
        make_test_case("test-long-a", &prompt_a),
        make_test_case("test-long-b", &prompt_b),
    ]);

    // Trace only has one episode with prompt_a
    let trace = create_trace_file(&[("ep-1", &prompt_a)]);

    let err = verify_coverage(trace.path(), &cfg).expect_err(
        "Truncation-shape matches MUST fail verification; never turn prefix match into verified coverage",
    );
    let msg = format!("{:#}", err);

    assert!(
        msg.contains("matches stage-local truncation shape")
            || msg.contains("matches the truncation shape"),
        "Must diagnose truncation shape match: {}",
        msg
    );
    assert!(
        msg.contains("exact prompt coverage cannot be established"),
        "Must name uncertainty: {}",
        msg
    );
    assert!(
        msg.contains("test-long-a") && msg.contains("test-long-b"),
        "Both distinct tests sharing the truncated shape must be reported: {}",
        msg
    );
}

#[test]
fn test_verify_coverage_mixed_exact_shape_and_missing() {
    let chunk = "✨ long prompt payload: ";
    let repeat_count = (5000 / chunk.len()) + 1;
    let long_prompt = chunk.repeat(repeat_count);

    let cfg = make_config(vec![
        make_test_case("test-exact", "Exact match prompt"),
        make_test_case("test-truncated-shape", &long_prompt),
        make_test_case("test-genuinely-missing", "Never appeared anywhere"),
    ]);

    let trace = create_trace_file(&[("ep-1", "Exact match prompt"), ("ep-2", &long_prompt)]);

    let err = verify_coverage(trace.path(), &cfg)
        .expect_err("Mixed verification with truncation-shape and missing MUST fail");
    let msg = format!("{:#}", err);

    // Missing prompt reported
    assert!(
        msg.contains("missing matching prompt in trace"),
        "Must report missing section: {}",
        msg
    );
    assert!(
        msg.contains("test-genuinely-missing"),
        "Must name missing test: {}",
        msg
    );

    // Truncation shape reported
    assert!(
        msg.contains("matches stage-local truncation shape")
            || msg.contains("matches the truncation shape"),
        "Must report truncation shape section: {}",
        msg
    );
    assert!(
        msg.contains("test-truncated-shape"),
        "Must name truncation-shape test: {}",
        msg
    );

    // Exact match is not in either error category
    assert!(
        !msg.contains("test-exact"),
        "Exact match must not appear in failures: {}",
        msg
    );
}
