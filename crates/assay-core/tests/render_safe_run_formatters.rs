//! MCP01a-5 in-slice conformance: the real `assay run` formatters render untrusted model/agent
//! content sink-safe.
//!
//! This drives the actual `assay-core::report::{json,sarif,junit}` writers and the console
//! prompt-preview helper over a planted hostile corpus and proves, end to end, the render-safety
//! invariants for the Assay CLI sinks (the CLI analog of the Plimsoll render-safety proof):
//!   - hostile probes (secret / PII / terminal-control) are redacted or stripped, never raw;
//!   - control-strip precedes redaction (a control byte glued inside a token cannot hide it);
//!   - redact-before-truncate on the bounded console preview (a secret cannot survive as a prefix);
//!   - benign content under an untrusted key survives (no over-redaction);
//!   - assay-owned ids / suite / fingerprint stay byte-stable;
//!   - structured sinks stay valid (serde-escaped, never double-encoded);
//!   - a clean (benign) row introduces no redaction markers.

use assay_core::model::{TestResultRow, TestStatus};
use assay_core::render_safety::{render_truncate_first_unsafe, Sink};
use assay_core::report::console::console_prompt_preview;
use assay_core::report::json::render_json;
use assay_core::report::junit::write_junit;
use assay_core::report::sarif::write_sarif;
use assay_core::report::RunArtifacts;

const EMAIL: &str = "alice@example.com";
const BENIGN: &str = "uuid 123e4567-e89b-12d3-a456-426614174000";
const ANSI: &str = "\u{1b}[31mRED\u{1b}[0m";
const OWNED_FP: &str = "ownedfp123";
const SUITE: &str = "owned-suite";

fn secret() -> String {
    format!("ghp_{}", "A".repeat(36))
}

/// Secret with a terminal-control sequence glued INSIDE the token: only control-strip-before-redact
/// re-forms `ghp_…` so the rule fires. The anti-evasion case.
fn secret_control_glued() -> String {
    format!("ghp\u{1b}[0m_{}", "A".repeat(36))
}

/// A prompt whose secret straddles the console 100-char preview boundary: redact-first kills it,
/// truncate-first would cut it into a non-matching `ghp_` fragment that leaks.
fn straddling_prompt() -> String {
    format!("{} {} tail", "x".repeat(90), secret())
}

fn row(
    test_id: &str,
    status: TestStatus,
    message: String,
    details: serde_json::Value,
) -> TestResultRow {
    TestResultRow {
        test_id: test_id.to_string(),
        status,
        score: Some(0.0),
        cached: false,
        message,
        details,
        duration_ms: Some(5),
        fingerprint: Some(OWNED_FP.to_string()),
        skip_reason: None,
        attempts: None,
        error_policy_applied: None,
    }
}

fn hostile_artifacts() -> RunArtifacts {
    let fail = row(
        "t_fail",
        TestStatus::Fail,
        format!("fail {ANSI} {EMAIL}"),
        serde_json::json!({
            "prompt": straddling_prompt(),
            "response": secret_control_glued(),
            "assertions": [{ "message": format!("got {}", secret()) }, { "passed": true }],
            "expected": BENIGN,
            "actual": format!("val {EMAIL}"),
            "skip": { "fingerprint": OWNED_FP, "reason": "fingerprint_match" },
            "owned_count": 7,
        }),
    );
    let error = row(
        "t_error",
        TestStatus::Error,
        format!("boom {}", secret()),
        serde_json::json!({}),
    );
    let pass = row(
        "t_pass",
        TestStatus::Pass,
        "ok".to_string(),
        serde_json::json!({ "prompt": "benign prompt", "response": "all good" }),
    );
    RunArtifacts {
        run_id: 1,
        suite: SUITE.to_string(),
        results: vec![fail, error, pass],
        order_seed: None,
        runner_clone_ms: None,
    }
}

fn assert_no_hostile_values(haystack: &str, ctx: &str) {
    assert!(!haystack.contains(&secret()), "{ctx}: raw secret leaked");
    assert!(!haystack.contains("ghp_"), "{ctx}: raw token prefix leaked");
    assert!(!haystack.contains(EMAIL), "{ctx}: raw pii leaked");
    assert!(
        !haystack.contains('\u{1b}'),
        "{ctx}: raw terminal control leaked"
    );
}

#[test]
fn run_json_record_sink_is_render_safe() {
    let artifacts = hostile_artifacts();
    let out = render_json(&artifacts).unwrap();

    // Structure preserved: still valid JSON (no double-encode / no broken serialization).
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();

    assert_no_hostile_values(&out, "run.json");
    assert!(out.contains("<redacted:"), "run.json fired no redaction");

    // Assay-owned values are byte-stable.
    assert!(out.contains(SUITE), "suite mutated");
    assert!(out.contains(OWNED_FP), "fingerprint mutated");
    assert_eq!(
        parsed["results"][0]["details"]["skip"]["fingerprint"],
        OWNED_FP
    );
    assert_eq!(
        parsed["results"][0]["details"]["skip"]["reason"],
        "fingerprint_match"
    );
    assert_eq!(parsed["results"][0]["details"]["owned_count"], 7);
    assert_eq!(parsed["results"][0]["test_id"], "t_fail");

    // Benign content under an untrusted key survives (no over-redaction).
    assert_eq!(parsed["results"][0]["details"]["expected"], BENIGN);

    // Anti-evasion: a control byte glued inside the token still redacts to a clean placeholder.
    assert_eq!(
        parsed["results"][0]["details"]["response"],
        "<redacted:github-token>"
    );

    // "Beyond the console's 100-char view": the record sink keeps full content, so the deep prompt
    // secret must be REDACTED here (not merely truncated away as on the console), and not truncated.
    let prompt = parsed["results"][0]["details"]["prompt"].as_str().unwrap();
    assert!(
        prompt.contains("<redacted:github-token>"),
        "deep prompt secret not redacted"
    );
    assert!(!prompt.contains("ghp_"));
    assert!(
        !prompt.contains("(truncated)"),
        "record sink must not truncate"
    );
}

#[test]
fn console_prompt_preview_redacts_before_truncating() {
    let prompt = straddling_prompt();
    let preview = console_prompt_preview(&prompt);

    assert!(!preview.contains("ghp_"), "console preview leaked secret");
    assert!(
        preview.contains("<redacted"),
        "console preview did not redact"
    );
    assert!(
        preview.contains("(truncated)"),
        "console preview did not bound"
    );

    // Differential: the OLD truncate-first order leaks a raw `ghp_` fragment at the same bound.
    let unsafe_preview = render_truncate_first_unsafe(Sink::Stdout, &prompt, 100);
    assert!(
        unsafe_preview.contains("ghp_"),
        "truncate-first is expected to leak (proves the order matters)"
    );
}

#[test]
fn sarif_report_sink_is_render_safe() {
    let artifacts = hostile_artifacts();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.sarif");
    write_sarif("assay", &artifacts.results, &path).unwrap();
    let s = std::fs::read_to_string(&path).unwrap();

    // Structure preserved.
    let _: serde_json::Value = serde_json::from_str(&s).unwrap();

    assert_no_hostile_values(&s, "sarif");
    assert!(s.contains("redacted"), "sarif fired no redaction");
    // Assay-owned test_id is rendered raw (serde-escaped), un-redacted.
    assert!(s.contains("t_fail"), "sarif dropped assay-owned test_id");
}

#[test]
fn junit_report_sink_is_render_safe_without_double_escape() {
    let artifacts = hostile_artifacts();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("out.xml");
    write_junit(SUITE, &artifacts.results, &path).unwrap();
    let j = std::fs::read_to_string(&path).unwrap();

    assert_no_hostile_values(&j, "junit");
    // The redaction placeholder is XML-escaped exactly once (`<` -> `&lt;`), never double-escaped
    // (`&amp;lt;`): the render-safety adapter performs the escape, replacing the plain escape().
    assert!(
        j.contains("&lt;redacted"),
        "junit redaction marker not xml-escaped"
    );
    assert!(
        !j.contains("&amp;lt;redacted"),
        "junit double-escaped the marker"
    );
    // Assay/config-owned suite + test_id render raw (XML-escaped only).
    assert!(j.contains(r#"name="owned-suite""#), "junit mutated suite");
    assert!(j.contains(r#"name="t_fail""#), "junit mutated test_id");
}

#[test]
fn benign_only_run_introduces_no_redaction_markers() {
    // Negative control / absence assertion: a clean run must not gain any redaction marker.
    let clean = RunArtifacts {
        run_id: 2,
        suite: SUITE.to_string(),
        results: vec![row(
            "t_clean",
            TestStatus::Fail,
            "expected greeting, got farewell".to_string(),
            serde_json::json!({ "prompt": "say hello", "response": "goodbye" }),
        )],
        order_seed: None,
        runner_clone_ms: None,
    };
    let out = render_json(&clean).unwrap();
    assert!(
        !out.contains("<redacted:"),
        "benign run over-redacted: {out}"
    );
    assert!(out.contains("say hello") && out.contains("goodbye"));
}
