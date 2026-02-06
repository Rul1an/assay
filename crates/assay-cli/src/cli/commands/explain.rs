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
use assay_core::explain;
use assay_evidence::lint::packs;
use clap::Args;
use std::collections::{BTreeMap, BTreeSet};
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

    /// Optional compliance pack used for article hints and coverage summary
    #[arg(long)]
    pub compliance_pack: Option<String>,
}

#[derive(Debug, Clone)]
struct ComplianceSummary {
    pack_name: String,
    applicable: usize,
    total: usize,
}

#[derive(Debug, Clone)]
struct ComplianceOutput {
    summary: ComplianceSummary,
    blocking_hints: Vec<(String, String)>,
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
    let compliance = if let Some(pack_ref) = args.compliance_pack.as_deref() {
        Some(build_compliance_output(&explanation, pack_ref)?)
    } else {
        None
    };

    // Format output
    let mut output = match args.format.as_str() {
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
    append_compliance_section(&mut output, args.format.as_str(), compliance.as_ref());

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

fn native_rule_article_ref(rule_id: &str) -> Option<&'static str> {
    if rule_id == "deny_list" {
        return Some("Article 15(3) - Robustness and accuracy");
    }
    if rule_id == "allow_list" {
        return Some("Article 12(1) - Record-keeping");
    }
    if rule_id.starts_with("max_calls_") {
        return Some("Article 14(4) - Human oversight");
    }
    if rule_id.starts_with("before_") {
        return Some("Article 12(2) - Traceability");
    }
    if rule_id.starts_with("never_after_") {
        return Some("Article 15(1) - Safety");
    }
    if rule_id.starts_with("sequence_") {
        return Some("Article 14(3) - Oversight");
    }
    None
}

fn article_for_rule(rule_id: &str, pack_articles: &BTreeMap<String, String>) -> Option<String> {
    if let Some(v) = pack_articles.get(rule_id) {
        return Some(v.clone());
    }
    native_rule_article_ref(rule_id).map(str::to_string)
}

fn build_compliance_output(
    explanation: &explain::TraceExplanation,
    pack_ref: &str,
) -> Result<ComplianceOutput> {
    let loaded = packs::load_pack(pack_ref)
        .with_context(|| format!("Failed to load compliance pack '{}'", pack_ref))?;

    let mut pack_articles = BTreeMap::new();
    for rule in &loaded.definition.rules {
        if let Some(article) = &rule.article_ref {
            pack_articles.insert(rule.id.clone(), article.clone());
        }
    }

    let mut evaluated_rule_ids = BTreeSet::new();
    for step in &explanation.steps {
        for eval in &step.rules_evaluated {
            evaluated_rule_ids.insert(eval.rule_id.clone());
        }
    }

    let applicable = evaluated_rule_ids
        .iter()
        .filter(|rid| article_for_rule(rid, &pack_articles).is_some())
        .count();

    let mut blocking_hints = Vec::new();
    for rid in &explanation.blocking_rules {
        if let Some(article) = article_for_rule(rid, &pack_articles) {
            blocking_hints.push((rid.clone(), article));
        }
    }

    Ok(ComplianceOutput {
        summary: ComplianceSummary {
            pack_name: loaded.definition.name,
            applicable,
            total: loaded.definition.rules.len(),
        },
        blocking_hints,
    })
}

fn append_compliance_section(
    output: &mut String,
    format: &str,
    compliance: Option<&ComplianceOutput>,
) {
    let Some(compliance) = compliance else {
        return;
    };

    // Keep JSON/HTML machine-formats untouched in this slice.
    if format == "json" || format == "html" {
        return;
    }

    let pct = if compliance.summary.total == 0 {
        0.0
    } else {
        (compliance.summary.applicable as f64 / compliance.summary.total as f64) * 100.0
    };

    if format == "markdown" || format == "md" {
        output.push_str("\n\n## Compliance Coverage\n");
        output.push_str(&format!(
            "- {}: {}/{} rules applicable ({:.1}%)\n",
            compliance.summary.pack_name,
            compliance.summary.applicable,
            compliance.summary.total,
            pct
        ));
        if !compliance.blocking_hints.is_empty() {
            output.push_str("\n### Blocking Rule Hints\n");
            for (rule, article) in &compliance.blocking_hints {
                output.push_str(&format!("- `{}` -> {}\n", rule, article));
            }
        }
        return;
    }

    output.push_str("\n\nCompliance Coverage:\n");
    output.push_str(&format!(
        "  {}: {}/{} rules applicable ({:.1}%)\n",
        compliance.summary.pack_name, compliance.summary.applicable, compliance.summary.total, pct
    ));
    if !compliance.blocking_hints.is_empty() {
        output.push_str("\nCompliance Hints:\n");
        for (rule, article) in &compliance.blocking_hints {
            output.push_str(&format!("  - {} -> {}\n", rule, article));
        }
    }
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

    #[test]
    fn test_article_for_rule_native_mapping() {
        let pack_articles = BTreeMap::new();
        assert!(article_for_rule("deny_list", &pack_articles)
            .unwrap()
            .contains("Article 15(3)"));
        assert!(article_for_rule("sequence_deploy_validate", &pack_articles)
            .unwrap()
            .contains("Article 14(3)"));
    }

    #[test]
    fn test_article_for_rule_pack_override() {
        let mut pack_articles = BTreeMap::new();
        pack_articles.insert("deny_list".to_string(), "12(9)".to_string());
        assert_eq!(
            article_for_rule("deny_list", &pack_articles).as_deref(),
            Some("12(9)")
        );
    }

    #[test]
    fn test_append_compliance_section_terminal() {
        let mut s = "base".to_string();
        let c = ComplianceOutput {
            summary: ComplianceSummary {
                pack_name: "eu-ai-act-baseline".to_string(),
                applicable: 3,
                total: 8,
            },
            blocking_hints: vec![(
                "deny_list".to_string(),
                "Article 15(3) - Robustness and accuracy".to_string(),
            )],
        };
        append_compliance_section(&mut s, "terminal", Some(&c));
        assert!(s.contains("Compliance Coverage:"));
        assert!(s.contains("3/8 rules applicable"));
        assert!(s.contains("deny_list -> Article 15(3)"));
    }
}
