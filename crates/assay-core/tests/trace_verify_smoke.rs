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
        !msg.contains("stage-local truncation shape")
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
        msg.contains("stage-local truncation shape")
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
        msg.contains("stage-local truncation shape")
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
    assert!(
        msg.contains("2 tests match stage-local truncation shape"),
        "plural grammar must be 'tests match', not 'tests matches': {}",
        msg
    );
    assert!(
        !msg.contains("2 tests matches stage-local truncation shape"),
        "must not emit 'tests matches': {}",
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
        msg.contains("stage-local truncation shape")
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

#[test]
fn test_verify_coverage_over_ceiling_absent_truncated_form() {
    // An over-4,096-byte prompt whose stage-local truncated form is ABSENT from the trace.
    // This tests the second conjunct in verify.rs:
    // `truncate_string(&mut expected, "prompt").is_some() && trace_prompts.contains(&expected)`
    // Deleting `&& trace_prompts.contains(...)` would falsely diagnose this absent prompt
    // as "matches stage-local truncation shape", when it is genuinely missing.
    let chunk = "unrelated over-ceiling configured prompt text: ";
    let repeat_count = (5000 / chunk.len()) + 1;
    let long_prompt = chunk.repeat(repeat_count);
    assert!(long_prompt.len() > 4096);

    let cfg = make_config(vec![make_test_case(
        "test-over-ceiling-absent",
        &long_prompt,
    )]);

    // Trace contains an episode, but with a completely different prompt
    let trace = create_trace_file(&[("ep-1", "A completely different short prompt")]);

    let err = verify_coverage(trace.path(), &cfg)
        .expect_err("Absent over-ceiling prompt must fail verification");
    let msg = format!("{:#}", err);

    // Must be reported as genuinely missing
    assert!(
        msg.contains("missing matching prompt in trace"),
        "Must report missing prompt: {}",
        msg
    );
    assert!(
        msg.contains("test-over-ceiling-absent"),
        "Must name the absent test: {}",
        msg
    );

    // Must NOT be reported as matching truncation shape
    assert!(
        !msg.contains("stage-local truncation shape")
            && !msg.contains("matches the truncation shape"),
        "Absent prompt must not be claimed to match truncation shape: {}",
        msg
    );
}

#[test]
fn test_verify_coverage_literal_marker_exact_match_control() {
    // A prompt whose content literally contains or ends with `...[TRUNCATED]`,
    // but is <= 4096 bytes and matches verbatim in the trace.
    // Suffix presence alone must NOT reject an exact match; truncation history is not
    // inferred from literal text.
    let literal_marker_prompt = "Query regarding logs ending with ...[TRUNCATED]";
    assert!(literal_marker_prompt.len() <= 4096);

    let cfg = make_config(vec![make_test_case(
        "test-literal-marker",
        literal_marker_prompt,
    )]);

    let trace = create_trace_file(&[("ep-1", literal_marker_prompt)]);

    let res = verify_coverage(trace.path(), &cfg);
    assert!(
        res.is_ok(),
        "Literal marker exact match must be accepted: {:?}",
        res.err()
    );
}

#[test]
fn test_verify_coverage_utf8_boundary_backoff() {
    // Measured constants: MAX_STRING_LEN = 4096, marker "...[TRUNCATED]" = 14 bytes.
    // Retained keep budget: 4096 - 14 = 4082 bytes.
    // Place a 4-byte UTF-8 character ('🦀') across byte index 4082.
    // 4081 ASCII bytes + '🦀' (bytes 4081..4085) + trailing bytes.
    // At keep = 4082, index 4082 is NOT a char boundary.
    // `truncate_string_to_byte_budget` must back off to byte 4081.
    // Emitted shape has 4081 bytes + 14-byte marker = 4095 bytes.
    let prefix = "a".repeat(4081);
    let split_prompt = format!("{}🦀{}", prefix, "z".repeat(100));
    assert!(split_prompt.len() > 4096);
    assert!(!split_prompt.is_char_boundary(4082));
    assert!(split_prompt.is_char_boundary(4081));

    let cfg = make_config(vec![make_test_case("test-utf8-backoff", &split_prompt)]);

    // Trace contains the un-truncated split_prompt. StreamUpgrader will truncate it
    // using the exact same backoff rule to 4081 bytes + marker.
    let trace = create_trace_file(&[("ep-utf8", &split_prompt)]);

    let err = verify_coverage(trace.path(), &cfg).expect_err(
        "Truncated shape match must fail coverage verification even with UTF-8 backoff",
    );
    let msg = format!("{:#}", err);

    assert!(
        msg.contains("stage-local truncation shape")
            || msg.contains("matches the truncation shape"),
        "Must diagnose truncation shape with UTF-8 backoff: {}",
        msg
    );
    assert!(
        msg.contains("test-utf8-backoff"),
        "Must name the backoff test: {}",
        msg
    );
}

#[test]
fn test_verify_coverage_exact_4096_through_stream_upgrader() {
    // Exactly MAX_STRING_LEN (4096) bytes: StreamUpgrader/truncation uses `len > 4096`,
    // so this prompt must remain verbatim and exact-match accept. A `>`→`>=` mutation
    // would truncate on ingest and make this fail — proving the threshold is load-bearing.
    let prompt = "E".repeat(4096);
    assert_eq!(prompt.len(), 4096);
    let cfg = make_config(vec![make_test_case("test-exact-4096", &prompt)]);
    let trace = create_trace_file(&[("ep-exact-4096", prompt.as_str())]);
    let res = verify_coverage(trace.path(), &cfg);
    assert!(
        res.is_ok(),
        "exact 4096-byte prompt through StreamUpgrader must remain accepted: {:?}",
        res.err()
    );
}
