//! `trace verify` consumes the ADR-050 observation carrier (#2782).
//!
//! Every test drives `verify_coverage_observed`, the additive entry point the CLI
//! routes through. The legacy `verify_coverage(path, cfg)` keeps its signature and
//! output; the last test pins that both agree on the coverage verdict.
//!
//! Readings are per yielded event occurrence and per producer-declared field root.
//! Trust is opt-in and empty by default; reported loss is never erased by trust; a
//! retained value carrying the in-band sentinel with no loss record is never raised
//! to measured-clean; `Unmeasured` is printed, never implied by omission.

use assay_core::model::{EvalConfig, Expected, Settings, TestCase, TestInput};
use assay_core::trace::observation::UPGRADER_STAGE;
use assay_core::trace::verify::{verify_coverage, verify_coverage_observed};
use serde_json::{json, Value};
use std::io::Write;
use tempfile::NamedTempFile;

const TRUSTED: &[&str] = &[UPGRADER_STAGE];
const FOREIGN: &[&str] = &["vendor.exporter"];

fn config(prompts: &[&str]) -> EvalConfig {
    EvalConfig {
        version: 2,
        suite: "verify_observations".to_string(),
        model: "test_model".to_string(),
        settings: Settings::default(),
        thresholds: Default::default(),
        otel: Default::default(),
        tests: prompts
            .iter()
            .enumerate()
            .map(|(i, prompt)| TestCase {
                id: format!("test-{i}"),
                input: TestInput {
                    prompt: (*prompt).to_string(),
                    context: None,
                },
                expected: Expected::MustContain {
                    must_contain: vec![],
                },
                assertions: None,
                on_error: None,
                tags: vec![],
                metadata: None,
            })
            .collect(),
    }
}

fn trace_file(lines: &[String]) -> NamedTempFile {
    let mut file = NamedTempFile::new().expect("temp trace");
    for line in lines {
        writeln!(file, "{line}").unwrap();
    }
    file.flush().unwrap();
    file
}

fn episode_start(episode_id: &str, input: Value, meta: Value) -> String {
    json!({
        "type": "episode_start",
        "episode_id": episode_id,
        "timestamp": 1,
        "input": input,
        "meta": meta
    })
    .to_string()
}

fn step(episode_id: &str, step_id: &str, content: &str) -> String {
    json!({
        "type": "step",
        "episode_id": episode_id,
        "step_id": step_id,
        "idx": 0,
        "timestamp": 2,
        "kind": "llm_completion",
        "name": "model",
        "content": content,
        "meta": null
    })
    .to_string()
}

fn tool_call(episode_id: &str, step_id: &str, call_index: Option<u32>) -> String {
    json!({
        "type": "tool_call",
        "episode_id": episode_id,
        "step_id": step_id,
        "timestamp": 3,
        "tool_name": "t",
        "call_index": call_index,
        "args": {},
        "result": null
    })
    .to_string()
}

fn episode_end(episode_id: &str) -> String {
    json!({
        "type": "episode_end",
        "episode_id": episode_id,
        "timestamp": 4,
        "outcome": "pass"
    })
    .to_string()
}

fn over_ceiling() -> String {
    "x".repeat(5000)
}

/// Run the observed entry point and return (verdict, captured output).
fn run(trace: &NamedTempFile, cfg: &EvalConfig, trusted: &[&str]) -> (anyhow::Result<()>, String) {
    let mut out = Vec::new();
    let result = verify_coverage_observed(trace.path(), cfg, trusted, &mut out);
    (result, String::from_utf8(out).expect("output is UTF-8"))
}

fn rows(output: &str) -> Vec<&str> {
    output
        .lines()
        .filter(|l| l.starts_with("truncation ordinal="))
        .collect()
}

fn assert_row(output: &str, expected: &str) {
    assert!(
        output.lines().any(|l| l == expected),
        "expected row\n  {expected}\nin output:\n{output}"
    );
}

fn full_episode() -> Vec<String> {
    vec![
        episode_start(
            "ep-1",
            json!({"prompt": "hello", "system": over_ceiling()}),
            json!({}),
        ),
        step("ep-1", "s1", "hi"),
        tool_call("ep-1", "s1", None),
        episode_end("ep-1"),
    ]
}

#[test]
fn default_trust_is_empty_and_every_field_root_is_printed() {
    let trace = trace_file(&full_episode());
    let (result, out) = run(&trace, &config(&["hello"]), &[]);
    assert!(result.is_ok(), "coverage must pass: {:?}", result.err());

    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=lossy",
    );
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/meta reading=unmeasured",
    );
    assert_row(
        &out,
        "truncation ordinal=2 kind=step episode_id=\"ep-1\" step_id=\"s1\" pointer=/content reading=unmeasured",
    );
    assert_row(
        &out,
        "truncation ordinal=2 kind=step episode_id=\"ep-1\" step_id=\"s1\" pointer=/meta reading=unmeasured",
    );
    // Optional call_index stays absent, never a fabricated zero.
    assert_row(
        &out,
        "truncation ordinal=3 kind=tool_call episode_id=\"ep-1\" step_id=\"s1\" pointer=/args reading=unmeasured",
    );
    assert_row(
        &out,
        "truncation ordinal=3 kind=tool_call episode_id=\"ep-1\" step_id=\"s1\" pointer=/result reading=unmeasured",
    );
    // EpisodeEnd advances the ordinal and gets no invented field reading.
    assert_row(
        &out,
        "truncation ordinal=4 kind=episode_end episode_id=\"ep-1\" fields=none",
    );
    assert_eq!(rows(&out).len(), 7, "{out}");
    assert!(
        !out.contains("measured_clean"),
        "an empty trust list must never license measured-clean:\n{out}"
    );
    assert!(
        out.contains("Trace Verification Passed"),
        "the coverage verdict follows the rows:\n{out}"
    );
}

#[test]
fn explicit_trust_licenses_that_stage_only_and_never_erases_loss() {
    let trace = trace_file(&full_episode());
    let cfg = config(&["hello"]);

    let (_, out) = run(&trace, &cfg, TRUSTED);
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=lossy",
    );
    let clean = format!("reading=measured_clean stage=\"{UPGRADER_STAGE}\" ceiling=4096");
    for pointer in [
        "ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/meta",
        "ordinal=2 kind=step episode_id=\"ep-1\" step_id=\"s1\" pointer=/content",
        "ordinal=2 kind=step episode_id=\"ep-1\" step_id=\"s1\" pointer=/meta",
        "ordinal=3 kind=tool_call episode_id=\"ep-1\" step_id=\"s1\" pointer=/args",
        "ordinal=3 kind=tool_call episode_id=\"ep-1\" step_id=\"s1\" pointer=/result",
    ] {
        assert_row(&out, &format!("truncation {pointer} {clean}"));
    }

    // An unrelated stage buys nothing.
    let (_, foreign) = run(&trace, &cfg, FOREIGN);
    assert!(!foreign.contains("measured_clean"), "{foreign}");
    assert!(
        foreign.contains("pointer=/input reading=lossy"),
        "{foreign}"
    );

    // Repeated trust values: the matching one licenses, the unrelated one is inert.
    let (_, both) = run(&trace, &cfg, &["vendor.exporter", UPGRADER_STAGE]);
    assert_eq!(rows(&both), rows(&out), "{both}");
}

#[test]
fn tool_call_with_call_index_prints_it() {
    let trace = trace_file(&[tool_call("ep-1", "s1", Some(2))]);
    let (_, out) = run(&trace, &config(&[]), &[]);
    assert_row(
        &out,
        "truncation ordinal=1 kind=tool_call episode_id=\"ep-1\" step_id=\"s1\" call_index=2 pointer=/args reading=unmeasured",
    );
}

#[test]
fn sentinel_without_loss_record_is_not_raised_to_clean() {
    let sentinel_prompt = format!("{}...[TRUNCATED]", "a".repeat(4000));
    let trace = trace_file(&[episode_start(
        "ep-1",
        json!({"prompt": sentinel_prompt}),
        json!({}),
    )]);
    let (_, out) = run(&trace, &config(&[&sentinel_prompt]), TRUSTED);
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=unmeasured",
    );
    // Sibling root without the sentinel still reads clean under the same trust.
    assert_row(
        &out,
        &format!(
            "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/meta reading=measured_clean stage=\"{UPGRADER_STAGE}\" ceiling=4096"
        ),
    );

    // Control: the same bytes without the sentinel read clean.
    let plain = "a".repeat(4000);
    let control = trace_file(&[episode_start("ep-1", json!({"prompt": plain}), json!({}))]);
    let (_, out) = run(&control, &config(&[&plain]), TRUSTED);
    assert_row(
        &out,
        &format!(
            "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=measured_clean stage=\"{UPGRADER_STAGE}\" ceiling=4096"
        ),
    );
}

#[test]
fn repeated_episode_ids_keep_distinct_occurrence_readings() {
    let trace = trace_file(&[
        episode_start("ep-dup", json!({"prompt": "same"}), json!({})),
        episode_start(
            "ep-dup",
            json!({"prompt": "same"}),
            json!({"note": over_ceiling()}),
        ),
    ]);
    let (result, out) = run(&trace, &config(&["same"]), &[]);
    assert!(result.is_ok(), "{:?}", result.err());
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-dup\" pointer=/meta reading=unmeasured",
    );
    assert_row(
        &out,
        "truncation ordinal=2 kind=episode_start episode_id=\"ep-dup\" pointer=/meta reading=lossy",
    );
}

#[test]
fn metadata_only_loss_with_matching_prompt_passes_coverage_and_reads_lossy() {
    let trace = trace_file(&[episode_start(
        "ep-1",
        json!({"prompt": "hello"}),
        json!({"a": {"b": over_ceiling()}}),
    )]);
    let (result, out) = run(&trace, &config(&["hello"]), TRUSTED);
    assert!(result.is_ok(), "{:?}", result.err());
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/meta reading=lossy",
    );
    assert_row(
        &out,
        &format!(
            "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=measured_clean stage=\"{UPGRADER_STAGE}\" ceiling=4096"
        ),
    );
}

#[test]
fn v1_record_expands_to_distinct_event_ordinals() {
    let v1 = json!({
        "schema_version": 1,
        "type": "assay.trace",
        "request_id": "r1",
        "prompt": "p",
        "response": "r"
    })
    .to_string();
    let trace = trace_file(&[v1, episode_start("ep-2", json!({"prompt": "q"}), json!({}))]);
    let (_, out) = run(&trace, &config(&["p", "q"]), &[]);
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"r1\" pointer=/input reading=unmeasured",
    );
    assert_row(
        &out,
        "truncation ordinal=2 kind=step episode_id=\"r1\" step_id=\"r1-step-0\" pointer=/content reading=unmeasured",
    );
    assert_row(
        &out,
        "truncation ordinal=3 kind=episode_end episode_id=\"r1\" fields=none",
    );
    assert_row(
        &out,
        "truncation ordinal=4 kind=episode_start episode_id=\"ep-2\" pointer=/input reading=unmeasured",
    );
}

#[test]
fn rows_precede_a_coverage_failure_and_unmeasured_is_not_omitted() {
    let trace = trace_file(&[episode_start(
        "ep-1",
        json!({"prompt": "present"}),
        json!({}),
    )]);
    let (result, out) = run(&trace, &config(&["absent"]), &[]);
    let err = format!("{:#}", result.expect_err("coverage must fail"));
    assert!(err.contains("missing matching prompt in trace"), "{err}");
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=unmeasured",
    );
    assert!(!out.contains("Trace Verification Passed"), "{out}");
}

#[test]
fn read_failure_after_a_valid_record_is_reported_as_incomplete() {
    let mut file = NamedTempFile::new().unwrap();
    writeln!(
        file,
        "{}",
        episode_start("ep-1", json!({"prompt": "hello"}), json!({}))
    )
    .unwrap();
    file.write_all(&[0xff, b'\n']).unwrap();
    file.flush().unwrap();

    let (result, out) = run(&file, &config(&["hello"]), &[]);
    assert!(result.is_err(), "an unreadable suffix is not EOF");
    assert_row(
        &out,
        "truncation ordinal=1 kind=episode_start episode_id=\"ep-1\" pointer=/input reading=unmeasured",
    );
    assert!(
        out.lines()
            .any(|l| l.starts_with("truncation incomplete after_ordinal=1 error=\"")),
        "a later read failure must be announced, not hidden behind printed rows:\n{out}"
    );
    assert!(!out.contains("Trace Verification Passed"), "{out}");
}

#[test]
fn identifiers_and_stages_are_escaped_to_one_ascii_line() {
    let hostile_id = "ep\"\\\n\u{2028}\u{202e}é🦀";
    let hostile_stage = "vendor\u{2028}x";
    let mut line: Value = serde_json::from_str(&episode_start(
        hostile_id,
        json!({"prompt": "hello"}),
        json!({"a": "b"}),
    ))
    .unwrap();
    // A trusted foreign observation with a loss under /meta and a clean /input.
    line["observations"] = json!([{
        "v": 1,
        "stage": hostile_stage,
        "ceiling": 10,
        "scope": ["/input", "/meta"],
        "losses": [{
            "field": "/meta/a",
            "original_len": 20,
            "kept_len": 10,
            "sha256": "00",
            "strategy": "head"
        }]
    }]);
    let trace = trace_file(&[line.to_string()]);
    let (_, out) = run(&trace, &config(&["hello"]), &[hostile_stage]);

    // The ASCII claim is about the rows; the pre-existing coverage line keeps its glyph.
    let rows = rows(&out);
    assert_eq!(rows.len(), 2, "{out}");
    for row in &rows {
        assert!(
            row.chars().all(|c| (' '..='~').contains(&c)),
            "rows must be printable ASCII only: {row:?}"
        );
    }
    let expected_id = "\"ep\\\"\\\\\\n\\u2028\\u202e\\u00e9\\ud83e\\udd80\"";
    assert_row(
        &out,
        &format!(
            "truncation ordinal=1 kind=episode_start episode_id={expected_id} pointer=/input reading=measured_clean stage=\"vendor\\u2028x\" ceiling=10"
        ),
    );
    assert_row(
        &out,
        &format!("truncation ordinal=1 kind=episode_start episode_id={expected_id} pointer=/meta reading=lossy"),
    );
    // The quoting is JSON string syntax, so a consumer can recover the original bytes.
    let decoded: String = serde_json::from_str(expected_id).unwrap();
    assert_eq!(decoded, hostile_id);
}

#[test]
fn legacy_entry_point_keeps_its_verdict_and_agrees_with_observed() {
    let cases: Vec<(Vec<String>, EvalConfig)> = vec![
        (
            vec![episode_start("e", json!({"prompt": "hello"}), json!({}))],
            config(&["hello"]),
        ),
        (
            vec![episode_start("e", json!({"prompt": "hello"}), json!({}))],
            config(&["absent"]),
        ),
        (
            vec![episode_start(
                "e",
                json!({"prompt": over_ceiling()}),
                json!({}),
            )],
            config(&[&over_ceiling()]),
        ),
        (full_episode(), config(&["hello", "absent"])),
    ];
    for (lines, cfg) in cases {
        let trace = trace_file(&lines);
        let legacy = verify_coverage(trace.path(), &cfg).map_err(|e| format!("{e:#}"));
        let (observed, _) = run(&trace, &cfg, TRUSTED);
        let observed = observed.map_err(|e| format!("{e:#}"));
        assert_eq!(legacy, observed, "trace: {lines:?}");
    }
}

#[test]
fn coverage_failure_report_cites_input_readings_per_occurrence() {
    use assay_core::trace::truncation::truncate_string;

    let long = over_ceiling();
    let mut truncated = long.clone();
    truncate_string(&mut truncated, "prompt").expect("over-ceiling prompt must truncate");
    assert!(
        truncated.contains("...[TRUNCATED]"),
        "the retained truncated form must carry the in-band sentinel"
    );

    let trace = trace_file(&[
        episode_start("ep-1", json!({"prompt": long.clone()}), json!({})),
        episode_start("ep-2", json!({"prompt": truncated}), json!({})),
    ]);
    let cfg = config(&[long.as_str()]);

    for trusted in [TRUSTED, &[] as &[&str]] {
        let (result, out) = run(&trace, &cfg, trusted);
        let err = format!(
            "{:#}",
            result.expect_err("coverage must fail: the exact prompt is absent")
        );
        // The verdict classification and wording are unchanged.
        assert!(
            err.contains("matches stage-local truncation shape"),
            "verdict wording must survive:\n{err}"
        );
        assert!(err.contains("     - test-0"), "{err}");
        let lossy = "       ordinal=1 /input reading=lossy";
        let unmeasured = "       ordinal=2 /input reading=unmeasured";
        for line in [lossy, unmeasured] {
            assert!(
                err.lines().any(|l| l == line),
                "expected occurrence line\n  {line}\nin report:\n{err}"
            );
        }
        let pos_lossy = err.lines().position(|l| l == lossy).unwrap();
        let pos_unmeasured = err.lines().position(|l| l == unmeasured).unwrap();
        assert!(
            pos_lossy < pos_unmeasured,
            "occurrences must stay in ordinal order:\n{err}"
        );
        assert!(!err.contains("Trace Verification Passed"), "{err}");
        if trusted.is_empty() {
            assert!(
                !err.contains("measured_clean"),
                "an empty trust list must never license measured-clean:\n{err}"
            );
            assert!(
                !out.contains("measured_clean"),
                "an empty trust list must never license measured-clean:\n{out}"
            );
        }
    }
}

#[test]
fn read_failure_on_the_first_record_reports_after_ordinal_zero() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all(&[0xff, b'\n']).unwrap();
    file.flush().unwrap();

    let (result, out) = run(&file, &config(&["hello"]), &[]);
    assert!(result.is_err(), "an unreadable first record is not EOF");
    assert_eq!(
        rows(&out).len(),
        0,
        "no event was yielded, so there are no rows:\n{out}"
    );
    assert!(
        out.lines()
            .any(|l| l.starts_with("truncation incomplete after_ordinal=0 error=\"")),
        "a first-record read failure must be announced at ordinal zero:\n{out}"
    );
    assert!(!out.contains("Trace Verification Passed"), "{out}");
}
