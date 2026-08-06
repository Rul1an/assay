//! Every `--format` argument must reject a value outside its documented set.
//!
//! Before this was enforced, each of these arguments was a bare `String` that clap accepted any
//! value for, and the command's `match` fell through to a `_` arm. In `evidence lint`, `evidence
//! diff` and `coverage` the structured branches write to stdout while the fallback writes only to
//! stderr, so a typo'd format produced an empty artifact with an unchanged exit code.

use std::process::Command;

fn help_for(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(args)
        .arg("--help")
        .output()
        .expect("failed to run assay");
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

/// One option's help text, joined across the lines clap wraps it over.
///
/// An option with a short flag (`explain -f, --format`) puts its description on the following
/// line, so matching a single line finds the flag and misses everything said about it.
fn option_block(help: &str, flag: &str) -> String {
    let lines: Vec<&str> = help.lines().collect();
    let start = lines
        .iter()
        .position(|l| l.contains(flag))
        .unwrap_or_else(|| panic!("no {flag} in help"));
    let mut block = lines[start].trim().to_string();
    for line in &lines[start + 1..] {
        let t = line.trim();
        // A new option starts the next block; a blank line ends the section.
        if t.is_empty() || t.starts_with('-') {
            break;
        }
        block.push(' ');
        block.push_str(t);
    }
    block
}

/// The accepted set of every format argument, as the CLI advertises it.
///
/// Each set is the union of the command's explicit match arms and its default, because in four of
/// these the default is only reachable through the fallback arm and would otherwise be rejected by
/// its own parser.
const FORMAT_ARGS: &[(&[&str], &str, &[&str])] = &[
    (
        &["evidence", "lint"],
        "--format",
        &["json", "sarif", "text"],
    ),
    (&["evidence", "diff"], "--format", &["human", "json"]),
    // `md` is accepted as an alias of `markdown` and deliberately not advertised, which is clap's
    // conventional treatment. `an_alias_is_accepted_without_being_advertised` covers it.
    (
        &["explain"],
        "--format",
        &["terminal", "markdown", "html", "json"],
    ),
    (
        &["profile", "show"],
        "--format",
        &["summary", "json", "yaml"],
    ),
    (&["mcp", "discover"], "--format", &["table", "json", "yaml"]),
    (&["sandbox"], "--profile-format", &["yaml", "json"]),
    // `github` is accepted as an alias of `markdown` and deliberately not advertised. `md` stays
    // advertised because it is the documented `--input` spelling, and legacy mode rejects it by
    // name rather than reinterpreting it, so it is not interchangeable with `markdown`.
    (
        &["coverage"],
        "--format",
        &["text", "json", "md", "markdown"],
    ),
    // Typed last (#2039). It was a bare `String` compared with `==`, and the final `else` wrote
    // JSON with a `// Default to JSON` comment, so a typo'd spelling produced a JSON file at the
    // markdown path at exit 0.
    (&["baseline", "report"], "--format", &["json", "md"]),
    (&["doctor"], "--format", &["text", "json"]),
];

/// Aliases that parse without appearing in the value list, as `(command, flag, alias)`.
const HIDDEN_ALIASES: &[(&[&str], &str, &str)] = &[
    (&["explain"], "--format", "md"),
    (&["coverage"], "--format", "github"),
];

#[test]
fn every_format_argument_advertises_its_accepted_values() {
    for (cmd, flag, values) in FORMAT_ARGS {
        let help = help_for(cmd);
        let line = option_block(&help, flag);
        assert!(
            line.contains("[possible values:"),
            "`assay {} {flag}` accepts any string: {line}",
            cmd.join(" "),
        );
        for v in *values {
            assert!(
                line.contains(v),
                "`assay {} {flag}` no longer accepts `{v}`: {line}",
                cmd.join(" "),
            );
        }
    }
}

/// Arguments whose default is not one value on one help line, and the test that covers them.
///
/// An entry here is a claim that the argument's default is verified somewhere stricter, not an
/// exemption: `every_listed_exception_names_a_live_test` fails if the named test stops existing.
const NO_SHARED_DEFAULT: &[(&[&str], &str, &str)] = &[(
    &["coverage"],
    "--format",
    "coverage_applies_its_own_default_in_each_mode",
)];

/// The default must be inside its own accepted set. Each of these defaults (`text`, `human`,
/// `summary`, `table`) is now a variant of the argument's enum rather than a bare string reaching a
/// fallback arm, so this holds the help line and the enum to the same answer.
#[test]
fn every_default_is_an_accepted_value() {
    for (cmd, flag, values) in FORMAT_ARGS {
        if NO_SHARED_DEFAULT
            .iter()
            .any(|(c, f, _)| c == cmd && f == flag)
        {
            continue;
        }
        let help = help_for(cmd);
        let line = option_block(&help, flag);
        let default = line
            .split("[default: ")
            .nth(1)
            .and_then(|s| s.split(']').next())
            .unwrap_or_else(|| panic!("`assay {} {flag}` has no default", cmd.join(" ")));
        assert!(
            values.contains(&default),
            "`assay {} {flag}` defaults to `{default}`, which is not in its own accepted set {values:?}",
            cmd.join(" "),
        );
    }
}

/// An argument may be skipped above only while the test named beside it exists.
///
/// Without this, deleting the per-mode test would silently turn the skip into a hole: the argument
/// would have no default checked anywhere and nothing would say so.
#[test]
fn every_listed_exception_names_a_live_test() {
    let source = include_str!("format_value_parser.rs");
    for (cmd, flag, test_name) in NO_SHARED_DEFAULT {
        assert!(
            source.contains(&format!("fn {test_name}(")),
            "`assay {} {flag}` is skipped in favour of `{test_name}`, which is not in this file",
            cmd.join(" "),
        );
    }
}

/// `coverage` has no shared default, because it is two commands behind one name and they do not
/// share an output kind. Each mode's default is checked by running that mode, not by reading one
/// help line — a single line cannot state two answers.
#[test]
fn coverage_applies_its_own_default_in_each_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let trace = dir.path().join("t.jsonl");
    std::fs::write(&trace, "{\"tool\":\"read\",\"args\":{}}\n").expect("write trace");

    // `--input` mode, no `--format`: the default is json, so the artifact parses as JSON.
    let out = dir.path().join("c.json");
    let status = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--input"])
        .arg(&trace)
        .arg("--out")
        .arg(&out)
        .args(["--declared-tool", "read"])
        .output()
        .expect("failed to run assay");
    assert!(
        status.status.success(),
        "`coverage --input` with no --format failed: {}",
        String::from_utf8_lossy(&status.stderr)
    );
    let written = std::fs::read_to_string(&out).expect("read report");
    serde_json::from_str::<serde_json::Value>(&written)
        .expect("`--input` mode's default did not produce JSON");

    // Legacy mode, no `--format`: the default is text, so stdout is the rendered report and not
    // JSON. Asserted on the artifact rather than on the help line, for the same reason as above.
    let policy = dir.path().join("p.yaml");
    std::fs::write(&policy, "version: 1\ntools:\n  allow: [\"read\"]\n").expect("write policy");
    let legacy_trace = dir.path().join("tr.jsonl");
    std::fs::write(&legacy_trace, "{\"tool_calls\":[{\"name\":\"read\"}]}\n").expect("write trace");
    let legacy = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--policy"])
        .arg(&policy)
        .arg("--trace-file")
        .arg(&legacy_trace)
        .output()
        .expect("failed to run assay");
    let stdout = String::from_utf8_lossy(&legacy.stdout);
    assert!(
        stdout.contains("Coverage Report"),
        "legacy mode's default did not produce the text report: {stdout}"
    );
    assert!(
        serde_json::from_str::<serde_json::Value>(&stdout).is_err(),
        "legacy mode's default produced JSON: {stdout}"
    );
}

/// Neither mode accepts a spelling only because the other mode's default requires it.
///
/// This is the defect that made the per-mode default necessary. `--format text` with `--input`
/// wrote JSON at exit 0 with no note that `text` was not honoured, because the shared default was
/// `text` and the mode had to accept it.
#[test]
fn neither_coverage_mode_accepts_the_others_default() {
    let dir = tempfile::tempdir().expect("tempdir");
    let trace = dir.path().join("t.jsonl");
    std::fs::write(&trace, "{\"tool\":\"read\",\"args\":{}}\n").expect("write trace");
    let out = dir.path().join("c.json");

    let rejected = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--input"])
        .arg(&trace)
        .arg("--out")
        .arg(&out)
        .args(["--declared-tool", "read", "--format", "text"])
        .output()
        .expect("failed to run assay");
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        !rejected.status.success(),
        "`--input --format text` succeeded; it used to write JSON and say nothing"
    );
    assert!(
        stderr.contains("--format text"),
        "the refusal does not name the spelling it refused: {stderr}"
    );
    assert!(
        !out.exists(),
        "a refused --format still wrote an artifact at {}",
        out.display()
    );

    // The mirror case, already true before this change and asserted here so the pair reads as one
    // rule rather than two coincidences.
    let policy = dir.path().join("p.yaml");
    std::fs::write(&policy, "version: 1\ntools:\n  allow: [\"read\"]\n").expect("write policy");
    let legacy_trace = dir.path().join("tr.jsonl");
    std::fs::write(&legacy_trace, "{\"tool_calls\":[{\"name\":\"read\"}]}\n").expect("write trace");
    let legacy = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["coverage", "--policy"])
        .arg(&policy)
        .arg("--trace-file")
        .arg(&legacy_trace)
        .args(["--format", "md"])
        .output()
        .expect("failed to run assay");
    assert!(
        !legacy.status.success(),
        "legacy `--format md` succeeded; it is the other mode's spelling"
    );
    assert!(
        String::from_utf8_lossy(&legacy.stderr).contains("--format md"),
        "the refusal does not name the spelling it refused"
    );
}

#[test]
fn an_unrecognized_format_is_rejected_rather_than_reinterpreted() {
    let out = Command::new(env!("CARGO_BIN_EXE_assay"))
        .args(["evidence", "lint", "/dev/null", "--format", "jsonn"])
        .output()
        .expect("failed to run assay");

    assert!(
        !out.status.success(),
        "a typo'd --format exited successfully",
    );
    assert!(
        out.stdout.is_empty(),
        "rejected run still wrote to stdout, which is where the artifact would go",
    );
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("invalid value 'jsonn'") && stderr.contains("possible values"),
        "the error does not name the value or the accepted set: {stderr}",
    );
}

/// Typing an argument turns an undocumented-but-accepted spelling into a declared alias. The alias
/// must keep working even though it no longer appears in the value list, or the change silently
/// breaks callers who relied on the old behaviour.
#[test]
fn an_alias_is_accepted_without_being_advertised() {
    for (cmd, flag, alias) in HIDDEN_ALIASES {
        let out = Command::new(env!("CARGO_BIN_EXE_assay"))
            .args(*cmd)
            .args([*flag, *alias])
            .output()
            .expect("failed to run assay");
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains("invalid value"),
            "`assay {} {flag} {alias}`: the alias was dropped rather than hidden: {stderr}",
            cmd.join(" "),
        );

        let line = option_block(&help_for(cmd), flag);
        assert!(
            !line.contains(&format!("{alias},")) && !line.contains(&format!("{alias}]")),
            "`assay {} {flag}` advertises `{alias}`, so it is a value and not a hidden alias: {line}",
            cmd.join(" "),
        );
    }
}
