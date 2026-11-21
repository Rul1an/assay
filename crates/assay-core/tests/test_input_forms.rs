//! The forms a test's `input` takes, through the eval config loader and the public type.
//!
//! `input` is a prompt string or a mapping with `prompt` and an optional `context` list. A key the
//! mapping does not know is named by the loader (a warning, or a refusal in strict mode) and left
//! out by the type; a value of another kind is refused.

use assay_core::config::{load_config_with, LoadOptions};
use assay_core::model::TestInput;

/// What a run gives: the prompt and context it read, or a part of the error it refused with.
#[derive(Debug)]
enum Seen {
    Read(String, Option<Vec<String>>),
    Refused(String),
}

fn through_the_loader(input: &str, strict: bool) -> Seen {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("eval.yaml");
    std::fs::write(
        &path,
        format!(
            "configVersion: 1\nsuite: s\nmodel: trace\ntests:\n  - id: t\n    input: {input}\n    expected: {{type: must_contain, must_contain: [hello]}}\n"
        ),
    )
    .expect("write");
    let options = LoadOptions {
        strict_unknown_fields: strict,
        ..Default::default()
    };
    match load_config_with(&path, options) {
        Ok(config) => {
            let TestInput { prompt, context } = config.tests[0].input.clone();
            Seen::Read(prompt, context)
        }
        Err(error) => Seen::Refused(format!("{error:#}")),
    }
}

fn through_the_type(yaml: &str) -> Seen {
    match serde_yaml::from_str::<TestInput>(yaml) {
        Ok(TestInput { prompt, context }) => Seen::Read(prompt, context),
        Err(error) => Seen::Refused(error.to_string()),
    }
}

/// Read with this prompt and context, or refused with an error containing this text.
enum Want {
    Read(&'static str, Option<&'static [&'static str]>),
    Refused(&'static str),
}

fn matches(seen: &Seen, want: &Want) -> bool {
    match (seen, want) {
        (Seen::Read(prompt, context), Want::Read(want_prompt, want_context)) => {
            prompt == want_prompt
                && context
                    .as_deref()
                    .map(|c| c.iter().map(String::as_str).collect::<Vec<_>>())
                    == want_context.map(|c| c.to_vec())
        }
        (Seen::Refused(error), Want::Refused(part)) => error.contains(part),
        _ => false,
    }
}

/// The shared grammar's name-or-mapping union says what it expected; before it, the decoder said
/// "expected string or struct TestInput".
const NOT_INPUT: &str = "expected a name, or a mapping";

#[test]
fn input_forms_through_the_loader_and_the_type() {
    use Want::{Read, Refused};
    let loader: Vec<(&str, bool, Want)> = vec![
        ("hello", true, Read("hello", None)),
        ("{prompt: hello}", true, Read("hello", None)),
        (
            "{prompt: hello, context: [a, b]}",
            true,
            Read("hello", Some(&["a", "b"])),
        ),
        ("{prompt: hello, context: null}", true, Read("hello", None)),
        (
            "{prompt: hello, extra: 1}",
            true,
            Refused("tests.0.input.extra"),
        ),
        ("{prompt: hello, extra: 1}", false, Read("hello", None)),
        ("\"123\"", true, Read("123", None)),
        ("yes", true, Read("yes", None)),
        ("123", true, Refused(NOT_INPUT)),
        ("true", true, Refused(NOT_INPUT)),
        ("[a]", true, Refused(NOT_INPUT)),
        ("null", true, Refused(NOT_INPUT)),
        ("!x hello", true, Refused(NOT_INPUT)),
        // A typed prompt takes the scalar's text; a bare `input: 123` above is refused.
        ("{prompt: 123}", true, Read("123", None)),
        ("{context: [a]}", true, Refused("missing field `prompt`")),
        (
            "{prompt: a, prompt: b}",
            true,
            Refused("duplicate field `prompt`"),
        ),
        ("{prompt: hello, context: a}", true, Refused("context")),
    ];
    let public: Vec<(&str, Want)> = vec![
        ("hello", Read("hello", None)),
        ("{prompt: hello, context: [a]}", Read("hello", Some(&["a"]))),
        ("{prompt: hello, extra: 1}", Read("hello", None)),
        ("123", Refused(NOT_INPUT)),
        ("[a]", Refused(NOT_INPUT)),
    ];

    let mut report = Vec::new();
    let mut failed = false;
    for (input, strict, want) in &loader {
        let seen = through_the_loader(input, *strict);
        failed |= !matches(&seen, want);
        report.push(format!("loader strict={strict} {input:?}: {seen:?}"));
    }
    for (yaml, want) in &public {
        let seen = through_the_type(yaml);
        failed |= !matches(&seen, want);
        report.push(format!("type {yaml:?}: {seen:?}"));
    }
    assert!(!failed, "{}", report.join("\n"));
}
