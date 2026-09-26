mod bindings;

use crate::cli::args::{Cli, DescribeArgs};
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_SUCCESS};
use anyhow::Result;
use clap::{Command, CommandFactory};
use serde::Serialize;
use std::io::{self, Write};

/// Document identity for the machine describe channel.
pub(crate) const DESCRIBE_REPORT_SCHEMA: &str = "assay.cli.describe.v0";

/// Machine-output selector longs `describe` reports, in canonical order.
///
/// Derived from the clap definitions: [`output_selectors`] reads the built
/// command, so a command that gains one of these is reported without a second
/// edit. Chosen over a hand-written per-command table because a table beside
/// the thing it describes drifts silently, in the dangerous direction — the
/// argument nobody listed is the one nobody checked.
pub(crate) const SELECTOR_LONGS: &[&str] = &["format", "json", "out", "output"];

#[derive(Serialize)]
struct DescribeReport {
    schema: &'static str,
    path: Vec<String>,
    commands: Vec<CommandEntry>,
    identities: Vec<&'static str>,
    selectors: Vec<String>,
}

#[derive(Serialize)]
struct CommandEntry {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<String>,
    has_children: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    format: Vec<String>,
    selectors: Vec<String>,
}

pub fn run(args: DescribeArgs) -> Result<i32> {
    let root = Cli::command();
    let Some(node) = resolve_node(&root, &args.path) else {
        return Ok(EXIT_CONFIG_ERROR);
    };
    let report = DescribeReport {
        schema: DESCRIBE_REPORT_SCHEMA,
        path: args.path.clone(),
        commands: visible_subcommands(node).map(command_entry).collect(),
        identities: bindings::identities_for(&args.path),
        selectors: output_selectors(node),
    };
    let mut stdout = io::stdout().lock();
    serde_json::to_writer(&mut stdout, &report)?;
    writeln!(stdout)?;
    Ok(EXIT_SUCCESS)
}

fn resolve_node<'a>(root: &'a Command, path: &[String]) -> Option<&'a Command> {
    let mut current = root;
    for (index, segment) in path.iter().enumerate() {
        match visible_subcommands(current).find(|child| child.get_name() == segment) {
            Some(child) => current = child,
            None => {
                let parent = if index == 0 {
                    "assay".to_string()
                } else {
                    format!("assay {}", path[..index].join(" "))
                };
                let children: Vec<&str> = visible_subcommands(current)
                    .map(Command::get_name)
                    .collect();
                let mut stderr = io::stderr().lock();
                let _ = writeln!(
                    stderr,
                    "error: unknown command path segment {segment:?} under {parent}"
                );
                if !children.is_empty() {
                    let _ = writeln!(stderr, "visible children: {}", children.join(", "));
                }
                return None;
            }
        }
    }
    Some(current)
}

fn visible_subcommands(cmd: &Command) -> impl Iterator<Item = &Command> {
    cmd.get_subcommands().filter(|child| !child.is_hide_set())
}

fn command_entry(cmd: &Command) -> CommandEntry {
    CommandEntry {
        name: cmd.get_name().to_string(),
        about: cmd.get_about().map(ToString::to_string),
        has_children: visible_subcommands(cmd).next().is_some(),
        format: format_values(cmd),
        selectors: output_selectors(cmd),
    }
}

/// Every machine-output selector the command accepts, read from its clap
/// definition. Additive to `format_values`, which keeps reporting the
/// accepted `--format` values unchanged.
fn output_selectors(cmd: &Command) -> Vec<String> {
    SELECTOR_LONGS
        .iter()
        .filter(|long| cmd.get_arguments().any(|arg| arg.get_long() == Some(*long)))
        .map(|long| format!("--{long}"))
        .collect()
}

fn format_values(cmd: &Command) -> Vec<String> {
    cmd.get_arguments()
        .find(|arg| arg.get_long() == Some("format"))
        .map(|arg| {
            arg.get_possible_values()
                .into_iter()
                .map(|value| value.get_name().to_string())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::{output_selectors, SELECTOR_LONGS};
    use crate::cli::args::Cli;
    use clap::{Command, CommandFactory};

    /// The vocabulary, stated a second time on purpose: the report above and
    /// this guard must agree, so editing one side's list without the other
    /// fails here rather than drifting silently.
    const EXPECTED_SELECTOR_LONGS: &[&str] = &["format", "json", "out", "output"];

    /// Longs that look output-ish but are deliberately not selectors: input
    /// formats and input toggles (`mcp-format`, `jsonl`), and differently
    /// spelled path args outside the S0 vocabulary (`out-md`, `out-dir`,
    /// `tool-decision-truth-out`, `otel-jsonl`, `profile-format`,
    /// `bundle-out`, `coverage-out`, `out-trace`, `state-window-out`).
    const KNOWN_NON_SELECTOR_OUTPUT_LONGS: &[&str] = &[
        "mcp-format",
        "jsonl",
        "otel-jsonl",
        "out-md",
        "out-dir",
        "tool-decision-truth-out",
        "profile-format",
        "bundle-out",
        "coverage-out",
        "out-trace",
        "state-window-out",
    ];

    fn looks_outputish(long: &str) -> bool {
        long.contains("json")
            || long.contains("format")
            || long == "out"
            || long == "output"
            || long.starts_with("out-")
            || long.starts_with("output-")
            || long.ends_with("-out")
            || long.ends_with("-output")
            || long.contains("-out-")
            || long.contains("-output-")
    }

    #[test]
    fn describe_reports_every_accepted_selector_and_nothing_else() {
        assert_eq!(
            SELECTOR_LONGS, EXPECTED_SELECTOR_LONGS,
            "the report vocabulary and the guard vocabulary must match; extend both"
        );
        let cli = Cli::command();
        let mut failures = Vec::new();
        let mut selector_args = 0usize;
        let mut commands = 0usize;
        walk(
            &cli,
            String::new(),
            &mut failures,
            &mut selector_args,
            &mut commands,
        );
        assert!(
            commands > 10,
            "the walk visited only {commands} commands; it is not reaching the tree"
        );
        assert!(
            selector_args > 20,
            "the walk found only {selector_args} selector args; it is not reaching them"
        );
        assert!(
            failures.is_empty(),
            "describe/clap selector drift:\n  {}",
            failures.join("\n  ")
        );
    }

    fn walk(
        cmd: &Command,
        path: String,
        failures: &mut Vec<String>,
        selector_args: &mut usize,
        commands: &mut usize,
    ) {
        *commands += 1;
        let expected: Vec<String> = EXPECTED_SELECTOR_LONGS
            .iter()
            .filter(|long| cmd.get_arguments().any(|arg| arg.get_long() == Some(*long)))
            .map(|long| format!("--{long}"))
            .collect();
        *selector_args += expected.len();
        let reported = output_selectors(cmd);
        if reported != expected {
            failures.push(format!(
                "assay {path}: describe reports {reported:?}, clap accepts {expected:?}"
            ));
        }
        for arg in cmd.get_arguments() {
            if let Some(long) = arg.get_long() {
                if looks_outputish(long)
                    && !EXPECTED_SELECTOR_LONGS.contains(&long)
                    && !KNOWN_NON_SELECTOR_OUTPUT_LONGS.contains(&long)
                {
                    failures.push(format!(
                        "assay {path}: new output-ish spelling --{long}; \
                         extend SELECTOR_LONGS (and this guard) or grandfather it with a reason"
                    ));
                }
            }
        }
        for sub in cmd.get_subcommands().filter(|child| !child.is_hide_set()) {
            let child_path = if path.is_empty() {
                sub.get_name().to_string()
            } else {
                format!("{path} {}", sub.get_name())
            };
            walk(sub, child_path, failures, selector_args, commands);
        }
    }
}
