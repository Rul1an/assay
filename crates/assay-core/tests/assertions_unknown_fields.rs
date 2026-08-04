//! An unrecognised key inside an `assertions:` entry must be rejected, not dropped (#1961).
//!
//! Serde drops an unknown key silently by default. Where the field the author meant has a
//! default, the assertion falls back to a shape that cannot fail — so a one-character typo
//! turns a real check into a permanent pass with no signal at any stage. The documented
//! `max_calls: 0` example is the worked case and inverts into its own opposite.
//!
//! These tests pin two things the fix depends on and one thing it must not break:
//!
//! 1. rejection reaches every variant, including `tool_blocklist`, whose fields are all
//!    defaulted so nothing but the tag is required — the shape closest to the unit-like
//!    variants that serde's known `deny_unknown_fields` defects are actually about;
//! 2. rejection survives the real config-loading path, which wraps deserialization in
//!    `serde_ignored` to collect unknown keys and, outside strict mode, only warns about
//!    them. A guard that fires on a bare `from_str` but not through `load_config` would be
//!    the check looking present while the CLI stayed permissive;
//! 3. free-form values nested under `policy` and `test_args` stay unconstrained.

use assay_core::agent_assertions::model::TraceAssertion;

fn parse(yaml: &str) -> Result<TraceAssertion, serde_yaml::Error> {
    serde_yaml::from_str(yaml)
}

/// Asserts rejection **and** that the message names the offending key.
///
/// A code-only assertion is too weak here: the whole point of the fix is that the author is
/// told which key was wrong, because the common case is one character away from a real field.
#[track_caller]
fn assert_rejects_naming(yaml: &str, offending_key: &str, case: &str) {
    match parse(yaml) {
        Ok(v) => panic!("{case}: expected rejection, but parsed into {v:?}"),
        Err(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains(offending_key),
                "{case}: rejected, but the message does not name `{offending_key}`: {msg}"
            );
        }
    }
}

#[track_caller]
fn assert_accepts(yaml: &str, case: &str) {
    if let Err(e) = parse(yaml) {
        panic!("{case}: expected this to parse, got {e}");
    }
}

// ---------------------------------------------------------------------------
// One misspelling per variant. The variants with a defaulted field are where a
// dropped key is dangerous; the all-required ones are covered so the guard is
// not silently variant-dependent.
// ---------------------------------------------------------------------------

#[test]
fn trace_must_call_tool_rejects_unknown_key() {
    assert_rejects_naming(
        "type: trace_must_call_tool\ntool: delete_database\nmax_calls: 0\n",
        "max_calls",
        "trace_must_call_tool / the documented max_calls inversion",
    );
    assert_rejects_naming(
        "type: trace_must_call_tool\ntool: web_search\nmin_call: 2\n",
        "min_call",
        "trace_must_call_tool / misspelled min_calls",
    );
    // The field name the documentation used for years.
    assert_rejects_naming(
        "type: trace_must_call_tool\ntool_name: web_search\n",
        "tool_name",
        "trace_must_call_tool / documented tool_name",
    );
}

#[test]
fn trace_must_not_call_tool_rejects_unknown_key() {
    assert_rejects_naming(
        "type: trace_must_not_call_tool\ntool: rm\nmax_calls: 0\n",
        "max_calls",
        "trace_must_not_call_tool / stray key",
    );
}

#[test]
fn trace_tool_sequence_rejects_unknown_key() {
    assert_rejects_naming(
        "type: trace_tool_sequence\nsequence: [a, b]\nallow_other_tool: true\n",
        "allow_other_tool",
        "trace_tool_sequence / misspelled allow_other_tools",
    );
    // `mode: loose` is what the architecture document wrote instead of the real field.
    assert_rejects_naming(
        "type: trace_tool_sequence\nsequence: [a, b]\nallow_other_tools: false\nmode: loose\n",
        "mode",
        "trace_tool_sequence / documented mode key",
    );
}

#[test]
fn trace_max_steps_rejects_unknown_key() {
    assert_rejects_naming(
        "type: trace_max_steps\nmax: 10\nsteps: 10\n",
        "steps",
        "trace_max_steps / stray key",
    );
}

#[test]
fn args_valid_rejects_unknown_key() {
    assert_rejects_naming(
        "type: args_valid\ntool: t\ntest_arg: {a: 1}\n",
        "test_arg",
        "args_valid / misspelled test_args",
    );
    assert_rejects_naming(
        "type: args_valid\ntool: t\ntest_args: {a: 1}\nexpekt: pass\n",
        "expekt",
        "args_valid / misspelled expect",
    );
}

#[test]
fn sequence_valid_rejects_unknown_key() {
    assert_rejects_naming(
        "type: sequence_valid\ntest_trace_row: []\n",
        "test_trace_row",
        "sequence_valid / misspelled test_trace_raw",
    );
}

/// The decisive variant: every field is defaulted, so only the tag is required. This is the
/// shape closest to the unit-like variants that serde's `deny_unknown_fields` issues describe,
/// and the one that would silently keep dropping keys if container-level rejection did not
/// reach it.
#[test]
fn tool_blocklist_rejects_unknown_key_despite_having_no_required_field() {
    assert_rejects_naming(
        "type: tool_blocklist\nblocked: [rm]\n",
        "blocked",
        "tool_blocklist / `blocked` at the top level instead of inside policy",
    );
    assert_rejects_naming(
        "type: tool_blocklist\ntest_tool_call: [rm]\n",
        "test_tool_call",
        "tool_blocklist / misspelled test_tool_calls",
    );
}

// ---------------------------------------------------------------------------
// Positive controls. Rejecting unknown keys must not narrow what a valid
// assertion may say.
// ---------------------------------------------------------------------------

#[test]
fn every_variant_still_parses_in_its_documented_form() {
    for (case, yaml) in [
        (
            "trace_must_call_tool",
            "type: trace_must_call_tool\ntool: web_search\nmin_calls: 1\n",
        ),
        (
            "trace_must_call_tool without min_calls",
            "type: trace_must_call_tool\ntool: web_search\n",
        ),
        (
            "trace_must_not_call_tool",
            "type: trace_must_not_call_tool\ntool: delete_database\n",
        ),
        (
            "trace_tool_sequence",
            "type: trace_tool_sequence\nsequence: [search, summarize]\nallow_other_tools: true\n",
        ),
        ("trace_max_steps", "type: trace_max_steps\nmax: 10\n"),
        (
            "args_valid",
            "type: args_valid\ntool: t\ntest_args: {percent: 10}\npolicy: {schema: {}}\nexpect: pass\n",
        ),
        (
            "sequence_valid",
            "type: sequence_valid\ntest_trace_raw: [{tool: a}]\npolicy: {regex: '^a$'}\nexpect: pass\n",
        ),
        (
            "tool_blocklist",
            "type: tool_blocklist\ntest_tool_calls: [rm]\npolicy: {blocked: [rm]}\nexpect: fail\n",
        ),
        ("tool_blocklist bare", "type: tool_blocklist\n"),
    ] {
        assert_accepts(yaml, case);
    }
}

/// The guard covers an assertion's own field vocabulary, not the contents of the free-form
/// values it carries. A policy or an argument object may hold any keys at all — constraining
/// those is the policy schema's job, and tightening them here would break every real config.
#[test]
fn nested_free_form_values_stay_unconstrained() {
    assert_accepts(
        "type: args_valid\ntool: t\ntest_args: {anything: {nested: [1, 2]}, weird_key: true}\npolicy: {schema: {properties: {x: {type: number}}}, extra_policy_key: 1}\n",
        "args_valid / free-form nested keys",
    );
    assert_accepts(
        "type: tool_blocklist\npolicy: {blocked: [rm], unknown_policy_key: 1}\n",
        "tool_blocklist / free-form policy keys",
    );
}

// ---------------------------------------------------------------------------
// The boundary that matters: the real config-loading path.
// ---------------------------------------------------------------------------

fn write_config(dir: &std::path::Path, assertion_block: &str) -> std::path::PathBuf {
    let path = dir.join("eval.yaml");
    std::fs::write(
        &path,
        format!(
            "configVersion: 1\nsuite: unknown_field_probe\nmodel: dummy\ntests:\n  - id: t1\n    input: hello\n    assertions:\n{assertion_block}"
        ),
    )
    .expect("write config");
    path
}

/// `load_config` deserializes through `serde_ignored`, which exists to *collect* unknown keys
/// rather than fail on them, and outside strict mode only prints a warning. If the guard were
/// swallowed there, it would fire in unit tests and never in the CLI — the check looking
/// present while every command stayed permissive.
///
/// Both modes are asserted: an unknown key inside an assertion is a hard error regardless of
/// `strict`, because unlike a stray top-level key it cannot be a forward-compatible extension.
#[test]
fn load_config_rejects_an_unknown_key_inside_an_assertion() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_config(
        tmp.path(),
        "      - type: trace_must_call_tool\n        tool: delete_database\n        max_calls: 0\n",
    );

    for strict in [false, true] {
        let err = assay_core::config::load_config(&path, false, strict)
            .expect_err(&format!("strict={strict}: expected load_config to reject"));
        let msg = err.to_string();
        assert!(
            msg.contains("max_calls"),
            "strict={strict}: rejected but did not name the offending key: {msg}"
        );
    }
}

#[test]
fn load_config_still_accepts_a_well_formed_assertion() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let path = write_config(
        tmp.path(),
        "      - type: trace_must_call_tool\n        tool: web_search\n        min_calls: 1\n",
    );
    assay_core::config::load_config(&path, false, false).expect("well-formed assertion must load");
}

/// The shipped suites are the check that this change does not break configurations that run
/// today (#1961 acceptance: "No first-party fixture or generated config fails the new check").
#[test]
fn first_party_suites_still_load() {
    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("repo root");

    for name in ["tests/fp_suite.yaml", "tests/regex_compatibility.yaml"] {
        let path = repo_root.join(name);
        if !path.exists() {
            continue;
        }
        assay_core::config::load_config(&path, false, false).unwrap_or_else(|e| {
            panic!("{name} no longer loads after the unknown-field guard: {e}")
        });
    }
}
