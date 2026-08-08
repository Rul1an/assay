//! An assertion that cannot fail is refused at load, and `allow_ineffective_assertions` is the way out.
//!
//! #1949 phased this: `assay validate` warned from #1983, `--deny-ineffective-assertions` made the
//! refusal available as an opt-in, and 5.0.0 is the major that carries the default. The pairing is
//! what these tests hold, because either half alone can pass while the gate is wrong. A gate that
//! refuses everything proves nothing, and a gate nothing reaches is not on.
//!
//! The behaviour change is deliberate and is pinned here rather than left to be discovered:
//! `load_config` acquires the refusal too. It is the convenience wrapper over the same
//! `LoadOptions`, so "every existing caller keeps its behaviour" stopped being true the moment the
//! default flipped, which is what a major is for.

use assay_core::config::{load_config, load_config_with, LoadOptions};
use std::io::Write;

fn suite(assertions: &str) -> tempfile::NamedTempFile {
    let mut f = tempfile::Builder::new().suffix(".yaml").tempfile().unwrap();
    write!(
        f,
        r#"
configVersion: 1
suite: ineffective_probe
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

/// The escape hatch, spelled out once so a reader does not have to infer the polarity.
fn allowed() -> LoadOptions {
    LoadOptions {
        allow_ineffective_assertions: true,
        ..Default::default()
    }
}

#[test]
fn an_assertion_that_cannot_fail_is_refused_without_asking() {
    let f = suite(INEFFECTIVE);
    let err = load_config_with(f.path(), LoadOptions::default())
        .expect_err("min_calls: 0 must be refused by default");
    let msg = err.to_string();
    assert!(msg.contains("cannot fail"), "{msg}");
    assert!(
        msg.contains("min_calls"),
        "must name the responsible field: {msg}"
    );
    assert!(msg.contains("t1"), "must locate the test: {msg}");
    assert!(
        msg.contains("--allow-ineffective-assertions"),
        "a refusal that does not name its own escape hatch is a dead end: {msg}"
    );
}

#[test]
fn the_convenience_wrapper_acquires_the_refusal_too() {
    // This is the breaking half of #1949 and the reason it waited for a major. `load_config` builds
    // `LoadOptions` with `..Default::default()`, so flipping the default reaches it by construction.
    // Pinned so that a later attempt to "restore compatibility" here has to argue with a test
    // rather than quietly reopen the hole.
    let f = suite(INEFFECTIVE);
    load_config(f.path(), false, false).expect_err("the wrapper must refuse it as well");
    load_config(f.path(), false, true)
        .expect_err("strict-unknown-fields does not change this axis");
}

#[test]
fn the_escape_hatch_still_loads_the_same_suite() {
    let f = suite(INEFFECTIVE);
    load_config_with(f.path(), allowed())
        .expect("--allow-ineffective-assertions must still run the suite");
}

#[test]
fn an_effective_assertion_loads_under_the_default() {
    // Rules out a gate that refuses everything and therefore proves nothing.
    let f = suite(EFFECTIVE);
    load_config_with(f.path(), LoadOptions::default())
        .expect("an assertion that can fail must load");
    load_config(f.path(), false, false).expect("and through the wrapper");
}

#[test]
fn the_refusal_does_not_echo_configured_values() {
    // Diagnostics stay value-free (#1949 item 4): naming the field is help, echoing the value is a
    // leak of suite content into whatever reads the error.
    let f = suite("      - type: trace_must_not_call_tool\n        tool: \"\"\n");
    let msg = load_config_with(f.path(), LoadOptions::default())
        .expect_err("an empty tool name cannot fail")
        .to_string();
    assert!(msg.contains("trace_must_not_call_tool"), "{msg}");
    assert!(!msg.contains("hello"), "must not echo suite content: {msg}");
}
