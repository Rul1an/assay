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
    (
        &["explain"],
        "--format",
        &["terminal", "markdown", "md", "html", "json"],
    ),
    (
        &["profile", "show"],
        "--format",
        &["summary", "json", "yaml"],
    ),
    (&["mcp", "discover"], "--format", &["table", "json", "yaml"]),
    (&["sandbox"], "--profile-format", &["yaml", "json"]),
    (
        &["coverage"],
        "--format",
        &["text", "json", "md", "markdown", "github"],
    ),
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

/// The default must be inside its own accepted set. Four of these defaults (`text`, `human`,
/// `summary`, `table`) have no match arm of their own and are only reachable through the fallback,
/// so a parser derived from the arms alone would reject the command's own default.
#[test]
fn every_default_is_an_accepted_value() {
    for (cmd, flag, values) in FORMAT_ARGS {
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
