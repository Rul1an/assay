//! `LoadOptions::deny_ineffective_assertions` refuses at load what `assay validate` warns about.
//!
//! The pairing matters more than either case alone: the same suite must load without the opt-in and
//! be refused with it, or the flag is not opt-in. And the refusal must name the ineffective field
//! rather than the configured value, because a diagnostic that echoes values leaks suite content
//! into CI logs.

use assay_core::config::{load_config, load_config_with, LoadOptions};
use std::io::Write;

fn suite(assertions: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(".yaml").tempfile().unwrap();
    write!(
        f,
        r#"
configVersion: 1
suite: deny_ineffective_probe
model: dummy
tests:
  - id: t1
    input: hello
    assertions:
{assertions}
"#
    )
    .unwrap();
    f.flush().unwrap();
    f
}

const INEFFECTIVE: &str =
    "      - type: trace_must_call_tool\n        tool: search\n        min_calls: 0\n";
const EFFECTIVE: &str =
    "      - type: trace_must_call_tool\n        tool: search\n        min_calls: 1\n";

fn deny() -> LoadOptions {
    LoadOptions {
        deny_ineffective_assertions: true,
        ..Default::default()
    }
}

#[test]
fn an_assertion_that_cannot_fail_is_refused_at_load_when_denied() {
    let f = suite(INEFFECTIVE);
    let err = load_config_with(f.path(), deny()).expect_err("min_calls: 0 must be refused");
    let msg = err.to_string();
    assert!(msg.contains("cannot fail"), "{msg}");
    assert!(
        msg.contains("min_calls"),
        "must name the responsible field: {msg}"
    );
    assert!(msg.contains("t1"), "must locate the test: {msg}");
}

#[test]
fn the_same_suite_loads_when_the_option_is_off() {
    // If this ever fails, the gate stopped being opt-in and every existing caller acquired it.
    let f = suite(INEFFECTIVE);
    load_config_with(f.path(), LoadOptions::default()).expect("must still load without the opt-in");
    load_config(f.path(), false, false).expect("the compatibility wrapper must not acquire it");
    load_config(f.path(), false, true).expect("strict-unknown-fields is a different axis");
}

#[test]
fn an_effective_assertion_loads_under_the_same_option() {
    // Rules out a gate that refuses everything and therefore proves nothing.
    let f = suite(EFFECTIVE);
    load_config_with(f.path(), deny()).expect("an assertion that can fail must load");
}

#[test]
fn the_refusal_does_not_echo_configured_values() {
    // Diagnostics stay value-free (#1949 item 4): naming the field is help, echoing the value is a
    // leak of suite content into whatever reads the error.
    let f = suite("      - type: trace_must_not_call_tool\n        tool: \"\"\n");
    let msg = load_config_with(f.path(), deny())
        .expect_err("an empty tool name cannot fail")
        .to_string();
    assert!(msg.contains("trace_must_not_call_tool"), "{msg}");
    assert!(!msg.contains("hello"), "must not echo suite content: {msg}");
}
