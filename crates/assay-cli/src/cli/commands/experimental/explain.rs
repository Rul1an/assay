//! CLI command: assay explain
//!
//! Visualize how a trace is evaluated against a policy.
//!
//! Usage:
//!   assay explain --trace trace.json --policy policy.yaml [--format terminal|markdown|html]
//!
//! Examples:
//!   assay explain -t trace.json -p policy.yaml
//!   assay explain -t trace.json -p policy.yaml --format markdown > report.md
//!   assay explain -t trace.json -p policy.yaml --format html -o report.html

use anyhow::{Context, Result};
use assay_core::experimental::explain;
use clap::Args;
use std::path::PathBuf;

#[derive(Args, Debug)]
pub struct ExplainArgs {
    /// Trace file (JSON or JSONL format)
    #[arg(short, long)]
    pub trace: PathBuf,

    /// Policy file to evaluate against
    #[arg(short, long)]
    pub policy: PathBuf,

    /// Output format: terminal, markdown, html, json
    #[arg(short, long, default_value = "terminal")]
    pub format: String,

    /// Output file (default: stdout)
    #[arg(short, long)]
    pub output: Option<PathBuf>,

    /// Show only blocked steps
    #[arg(long)]
    pub blocked_only: bool,

    /// Show rule evaluation details for all steps
    #[arg(long)]
    pub verbose: bool,
}

/// Trace input formats
#[derive(Debug, serde::Deserialize)]
#[serde(untagged)]
enum TraceInput {
    /// Array of tool calls
    Array(Vec<ToolCallInput>),
    /// Object with tools field
    Object {
        #[serde(alias = "tool_calls", alias = "calls")]
        tools: Vec<ToolCallInput>,
    },
    /// OpenTelemetry-style spans
    OTelTrace { spans: Vec<OTelSpan> },
}

#[derive(Debug, serde::Deserialize)]
struct ToolCallInput {
    #[serde(alias = "name", alias = "tool_name")]
    tool: String,
    #[serde(default)]
    args: Option<serde_json::Value>,
    #[serde(default, alias = "arguments", alias = "parameters")]
    params: Option<serde_json::Value>,
}

#[derive(Debug, serde::Deserialize)]
struct OTelSpan {
    name: String,
    #[serde(default)]
    attributes: Option<serde_json::Value>,
}

impl ToolCallInput {
    fn into_tool_call(self) -> explain::ToolCall {
        explain::ToolCall {
            tool: self.tool,
            args: self.args.or(self.params),
        }
    }
}

pub async fn run(args: ExplainArgs) -> Result<i32> {
    // Load policy
    let policy_content = tokio::fs::read_to_string(&args.policy)
        .await
        .with_context(|| format!("Failed to read policy: {}", args.policy.display()))?;

    let policy: assay_core::model::Policy = serde_yaml::from_str(&policy_content)
        .with_context(|| format!("Failed to parse policy: {}", args.policy.display()))?;

    // Load trace
    let trace_content = tokio::fs::read_to_string(&args.trace)
        .await
        .with_context(|| format!("Failed to read trace: {}", args.trace.display()))?;

    let tool_calls = parse_trace(&trace_content)
        .with_context(|| format!("Failed to parse trace: {}", args.trace.display()))?;

    if tool_calls.is_empty() {
        eprintln!("Warning: Trace is empty");
    }

    // Run explanation
    let explainer = explain::TraceExplainer::new(policy);
    let explanation = explainer.explain(&tool_calls);

    // Format output
    let output = match args.format.as_str() {
        "markdown" | "md" => explanation.to_markdown(),
        "html" => explanation.to_html(),
        "json" => serde_json::to_string_pretty(&explanation)?,
        "terminal" => {
            if args.verbose {
                format_verbose(&explanation)
            } else if args.blocked_only {
                format_blocked_only(&explanation)
            } else {
                explanation.to_terminal()
            }
        }
        _ => {
            if args.verbose {
                format_verbose(&explanation)
            } else if args.blocked_only {
                format_blocked_only(&explanation)
            } else {
                explanation.to_terminal()
            }
        }
    };

    // Write output
    if let Some(output_path) = args.output {
        tokio::fs::write(&output_path, &output)
            .await
            .with_context(|| format!("Failed to write output: {}", output_path.display()))?;
        eprintln!("Wrote explanation to {}", output_path.display());
    } else {
        println!("{}", output);
    }

    // Exit code: 0 if all allowed, 1 if any blocked
    Ok(if explanation.blocked_steps > 0 { 1 } else { 0 })
}

fn parse_trace(content: &str) -> Result<Vec<explain::ToolCall>> {
    let content = content.trim();

    // Try parsing as JSON first (Array or Object or OTel)
    if let Ok(input) = serde_json::from_str::<TraceInput>(content) {
        return Ok(match input {
            TraceInput::Array(calls) => calls.into_iter().map(|c| c.into_tool_call()).collect(),
            TraceInput::Object { tools } => tools.into_iter().map(|c| c.into_tool_call()).collect(),
            TraceInput::OTelTrace { spans } => {
                // Convert OTel spans to tool calls
                spans
                    .into_iter()
                    .filter(|s| s.name.contains('.') || !s.name.starts_with("internal"))
                    .map(|s| explain::ToolCall {
                        tool: s.name,
                        args: s.attributes,
                    })
                    .collect()
            }
        });
    }

    // Try JSONL (one JSON object per line)
    let mut calls = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let input: ToolCallInput =
            serde_json::from_str(line).with_context(|| format!("Invalid JSON line: {}", line))?;
        calls.push(input.into_tool_call());
    }

    Ok(calls)
}

fn format_verbose(explanation: &explain::TraceExplanation) -> String {
    let mut lines = Vec::new();

    lines.push(format!(
        "Policy: {} (v{})",
        explanation.policy_name, explanation.policy_version
    ));
    lines.push(format!(
        "Trace: {} steps ({} allowed, {} blocked)\n",
        explanation.total_steps, explanation.allowed_steps, explanation.blocked_steps
    ));

    lines.push("Timeline:".to_string());
    lines.push(String::new());

    for step in &explanation.steps {
        let icon = match step.verdict {
            explain::StepVerdict::Allowed => "✅",
            explain::StepVerdict::Blocked => "❌",
            explain::StepVerdict::Warning => "⚠️",
        };

        lines.push(format!("─── Step {} ───", step.index));
        lines.push(format!("  Tool: {} {}", step.tool, icon));

        if let Some(args) = &step.args {
            lines.push(format!(
                "  Args: {}",
                serde_json::to_string(args).unwrap_or_default()
            ));
        }

        lines.push(format!("  Verdict: {:?}", step.verdict));
        lines.push(String::new());

        lines.push("  Rules Evaluated:".to_string());
        for eval in &step.rules_evaluated {
            let status = if eval.passed { "✓" } else { "✗" };
            lines.push(format!(
                "    {} [{}] {}",
                status, eval.rule_type, eval.rule_id
            ));
            lines.push(format!("      {}", eval.explanation));
        }

        lines.push(String::new());
    }

    lines.join("\n")
}

fn format_blocked_only(explanation: &explain::TraceExplanation) -> String {
    let mut lines = Vec::new();

    if explanation.blocked_steps == 0 {
        lines.push("✅ All steps allowed".to_string());
        return lines.join("\n");
    }

    lines.push(format!(
        "❌ {} blocked step(s):\n",
        explanation.blocked_steps
    ));

    for step in &explanation.steps {
        if step.verdict != explain::StepVerdict::Blocked {
            continue;
        }

        lines.push(format!("[{}] {} ❌ BLOCKED", step.index, step.tool));

        for eval in &step.rules_evaluated {
            if !eval.passed {
                lines.push(format!("    Rule: {}", eval.rule_id));
                lines.push(format!("    Reason: {}", eval.explanation));
            }
        }

        lines.push(String::new());
    }

    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_json_array() {
        let content = r#"[
            {"tool": "Search", "args": {"query": "test"}},
            {"tool": "Create"}
        ]"#;

        let calls = parse_trace(content).unwrap();
        assert_eq!(calls.len(), 2);
        assert_eq!(calls[0].tool, "Search");
        assert_eq!(calls[1].tool, "Create");
    }

    #[test]
    fn test_parse_json_object() {
        let content = r#"{
            "tools": [
                {"tool": "Search"},
                {"tool": "Create"}
            ]
        }"#;

        let calls = parse_trace(content).unwrap();
        assert_eq!(calls.len(), 2);
    }

    #[test]
    fn test_parse_jsonl() {
        let content = r#"{"tool": "Search"}
{"tool": "Create"}
{"tool": "Update"}"#;

        let calls = parse_trace(content).unwrap();
        assert_eq!(calls.len(), 3);
    }

    #[test]
    fn test_parse_with_aliases() {
        let content = r#"[
            {"name": "Search"},
            {"tool_name": "Create"}
        ]"#;

        let calls = parse_trace(content).unwrap();
        assert_eq!(calls[0].tool, "Search");
        assert_eq!(calls[1].tool, "Create");
    }
}
