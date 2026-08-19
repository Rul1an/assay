mod bindings;

use crate::cli::args::{Cli, DescribeArgs};
use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_SUCCESS};
use anyhow::Result;
use clap::{Command, CommandFactory};
use serde::Serialize;
use std::io::{self, Write};

/// Document identity for the machine describe channel.
pub(crate) const DESCRIBE_REPORT_SCHEMA: &str = "assay.cli.describe.v0";

#[derive(Serialize)]
struct DescribeReport {
    schema: &'static str,
    path: Vec<String>,
    commands: Vec<CommandEntry>,
    identities: Vec<&'static str>,
}

#[derive(Serialize)]
struct CommandEntry {
    name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    about: Option<String>,
    has_children: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    format: Vec<String>,
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
    }
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
