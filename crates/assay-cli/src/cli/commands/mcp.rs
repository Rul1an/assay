use super::super::args::{McpArgs, McpSub, McpWrapArgs};
use super::coverage;
use super::session_state_window;
use anyhow::{Context, Result};
use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::proxy::{McpProxy, ProxyConfig, ProxyConfigRaw};
use serde_json::{json, Value};
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

pub async fn run(args: McpArgs) -> anyhow::Result<i32> {
    match args.cmd {
        McpSub::Wrap(wrap_args) => cmd_wrap(wrap_args).await,
        McpSub::ConfigPath(config_args) => {
            super::config_path::run(config_args);
            Ok(0)
        }
    }
}

fn is_explicit_tool_name(pattern: &str) -> bool {
    let trimmed = pattern.trim();
    !trimmed.is_empty() && !trimmed.contains('*')
}

fn collect_declared_tools(policy: &McpPolicy) -> Vec<String> {
    let mut declared = BTreeSet::new();

    if let Some(allow) = &policy.tools.allow {
        for tool in allow {
            if is_explicit_tool_name(tool) {
                declared.insert(tool.trim().to_string());
            }
        }
    }

    if let Some(deny) = &policy.tools.deny {
        for tool in deny {
            if is_explicit_tool_name(tool) {
                declared.insert(tool.trim().to_string());
            }
        }
    }

    for tool in policy.schemas.keys() {
        if tool != "$defs" && is_explicit_tool_name(tool) {
            declared.insert(tool.trim().to_string());
        }
    }

    for constraint in &policy.constraints {
        if is_explicit_tool_name(&constraint.tool) {
            declared.insert(constraint.tool.trim().to_string());
        }
    }

    for tool in policy.tool_pins.keys() {
        if is_explicit_tool_name(tool) {
            declared.insert(tool.trim().to_string());
        }
    }

    declared.into_iter().collect()
}

fn extract_tool_name_from_decision_event(v: &Value) -> Result<String> {
    for (pointer, label) in [
        (Some("/data/tool"), "data.tool"),
        (Some("/data/tool_name"), "data.tool_name"),
        (None, "tool"),
        (None, "tool_name"),
    ] {
        let value = match pointer {
            Some(path) => v.pointer(path),
            None => v.get(label),
        };
        if let Some(s) = value.and_then(Value::as_str) {
            let tool = s.trim();
            if !tool.is_empty() {
                return Ok(tool.to_string());
            }
            anyhow::bail!("field '{label}' must be non-empty string");
        }
    }

    anyhow::bail!("missing required field: 'data.tool', 'data.tool_name', 'tool', or 'tool_name'")
}

fn extract_tool_classes_from_decision_event(v: &Value) -> BTreeSet<String> {
    match v.pointer("/data/tool_classes") {
        Some(Value::Array(arr)) => arr
            .iter()
            .filter_map(Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(ToOwned::to_owned)
            .collect(),
        _ => BTreeSet::new(),
    }
}

async fn normalize_decision_jsonl_to_coverage_jsonl(input: &Path, output: &Path) -> Result<()> {
    let content = tokio::fs::read_to_string(input)
        .await
        .with_context(|| format!("failed to read decision log {}", input.display()))?;

    let mut lines = Vec::new();
    for (lineno, raw) in content.lines().enumerate() {
        let line = raw.trim();
        if line.is_empty() {
            continue;
        }

        let value: Value = serde_json::from_str(line)
            .with_context(|| format!("invalid json at line {}", lineno + 1))?;
        let tool = extract_tool_name_from_decision_event(&value)
            .with_context(|| format!("measurement error at line {}", lineno + 1))?;
        let tool_classes = extract_tool_classes_from_decision_event(&value)
            .into_iter()
            .collect::<Vec<_>>();

        lines.push(serde_json::to_string(&json!({
            "tool": tool,
            "tool_classes": tool_classes,
        }))?);
    }

    let mut normalized = lines.join("\n");
    if !normalized.is_empty() {
        normalized.push('\n');
    }

    tokio::fs::write(output, normalized)
        .await
        .with_context(|| {
            format!(
                "failed to write normalized coverage input {}",
                output.display()
            )
        })?;
    Ok(())
}

fn unique_temp_path(stem: &str, extension: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "assay-{stem}-{}-{stamp}.{extension}",
        std::process::id()
    ))
}

fn generate_session_id() -> String {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("mcpwrap-{}-{stamp}", std::process::id())
}

struct TempPathGuard {
    path: PathBuf,
}

impl TempPathGuard {
    fn new(path: PathBuf) -> Self {
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempPathGuard {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.path);
    }
}

async fn cmd_wrap(args: McpWrapArgs) -> anyhow::Result<i32> {
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    if args.command.is_empty() {
        anyhow::bail!("No command specified. Usage: assay mcp wrap -- <cmd> [args]");
    }

    let cmd = &args.command[0];
    let cmd_args = &args.command[1..];
    let session_id = generate_session_id();

    // Load Policy
    let policy = if args.policy.exists() {
        eprintln!("[assay] loading policy from {}", args.policy.display());
        McpPolicy::from_file(&args.policy)?
    } else {
        anyhow::bail!(
            "Policy file not found at {}. Aborting instead of using insecure default (allow-all) policy.",
            args.policy.display()
        );
    };

    let declared_tools = if args.coverage_out.is_some() {
        collect_declared_tools(&policy)
    } else {
        Vec::new()
    };

    let temp_decision_log = if args.coverage_out.is_some() && args.decision_log.is_none() {
        Some(TempPathGuard::new(unique_temp_path(
            "wrap-decision-log",
            "ndjson",
        )))
    } else {
        None
    };
    let effective_decision_log = args.decision_log.clone().or_else(|| {
        temp_decision_log
            .as_ref()
            .map(|guard| guard.path().to_path_buf())
    });

    // Build and validate config (fail-fast on invalid event_source)
    let raw = ProxyConfigRaw {
        dry_run: args.dry_run,
        verbose: args.verbose,
        audit_log_path: args.audit_log.clone(),
        decision_log_path: effective_decision_log.clone(),
        event_source: args.event_source.clone(),
        server_id: args
            .label
            .clone()
            .unwrap_or_else(|| "default-mcp-server".into()),
    };
    let config = ProxyConfig::try_from_raw(raw)?;
    let state_window_event_source = config.event_source.clone();
    let state_window_server_id = config.server_id.clone();

    if config.dry_run {
        eprintln!("[assay] DRY RUN MODE: No actions will be blocked.");
    }

    eprintln!("[assay] wrapping command: {} {:?}", cmd, cmd_args);

    // Spawn proxy
    let proxy = McpProxy::spawn(cmd, cmd_args, policy, config)?;

    // Run (blocking for now, as it manages threads)
    let wrapped_code = tokio::task::spawn_blocking(move || proxy.run()).await??;

    let coverage_status = if let Some(coverage_out) = args.coverage_out.as_ref() {
        let normalized_log = TempPathGuard::new(unique_temp_path("wrap-coverage-input", "jsonl"));
        let decision_log_path = effective_decision_log
            .as_ref()
            .context("coverage generation requires a decision log path")?;

        match normalize_decision_jsonl_to_coverage_jsonl(decision_log_path, normalized_log.path())
            .await
        {
            Ok(()) => {
                coverage::write_generated_coverage_report(
                    normalized_log.path(),
                    coverage_out,
                    &declared_tools,
                    "decision_jsonl",
                )
                .await?
            }
            Err(e) => {
                eprintln!(
                    "Measurement error: failed to normalize decision log {}: {e}",
                    decision_log_path.display()
                );
                crate::exit_codes::EXIT_CONFIG_ERROR
            }
        }
    } else {
        crate::exit_codes::EXIT_SUCCESS
    };

    let state_status = if let Some(state_window_out) = args.state_window_out.as_ref() {
        let event_source = state_window_event_source
            .as_deref()
            .context("state window export requires event_source")?;
        session_state_window::write_state_window_out(
            state_window_out,
            event_source,
            &state_window_server_id,
            &session_id,
        )
        .await?
    } else {
        crate::exit_codes::EXIT_SUCCESS
    };

    if wrapped_code != crate::exit_codes::EXIT_SUCCESS {
        if coverage_status != crate::exit_codes::EXIT_SUCCESS
            || state_status != crate::exit_codes::EXIT_SUCCESS
        {
            eprintln!(
                "Coverage/state-window generation failed after wrapped command exited with code {}; preserving wrapped exit code",
                wrapped_code
            );
        }
        return Ok(wrapped_code);
    }

    if coverage_status != crate::exit_codes::EXIT_SUCCESS {
        if state_status != crate::exit_codes::EXIT_SUCCESS {
            eprintln!(
                "State window export failed after coverage generation failed; preserving coverage exit code"
            );
        }
        return Ok(coverage_status);
    }

    Ok(state_status)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../scripts/ci/fixtures/coverage")
            .join(name)
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_extracts_nested_tool_and_classes() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("normalized.jsonl");

        normalize_decision_jsonl_to_coverage_jsonl(
            &fixture_path("decision_event_basic.jsonl"),
            &out,
        )
        .await
        .expect("normalization should succeed");

        let content = std::fs::read_to_string(&out).unwrap();
        let line = content.lines().next().unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["tool"], "assay_policy_decide");
        assert_eq!(value["tool_classes"], json!(["sink:network"]));
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_accepts_data_tool_name_fallback() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("decision.jsonl");
        let out = dir.path().join("normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"data":{"tool_name":"web_search_alt"}}
"#,
        )
        .unwrap();

        normalize_decision_jsonl_to_coverage_jsonl(&input, &out)
            .await
            .expect("data.tool_name fallback should succeed");

        let content = std::fs::read_to_string(&out).unwrap();
        let line = content.lines().next().unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["tool"], "web_search_alt");
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_accepts_top_level_tool_name_fallback() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("decision.jsonl");
        let out = dir.path().join("normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"tool_name":"web_search"}
"#,
        )
        .unwrap();

        normalize_decision_jsonl_to_coverage_jsonl(&input, &out)
            .await
            .expect("top-level tool_name fallback should succeed");

        let content = std::fs::read_to_string(&out).unwrap();
        let line = content.lines().next().unwrap();
        let value: Value = serde_json::from_str(line).unwrap();
        assert_eq!(value["tool"], "web_search");
        assert_eq!(value["tool_classes"], json!([]));
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_rejects_missing_tool_fields() {
        let dir = tempdir().unwrap();
        let input = dir.path().join("decision.jsonl");
        let out = dir.path().join("normalized.jsonl");
        std::fs::write(
            &input,
            r#"{"decision":"deny"}
"#,
        )
        .unwrap();

        let err = normalize_decision_jsonl_to_coverage_jsonl(&input, &out)
            .await
            .expect_err("missing tool fields must fail");
        let msg = format!("{err:#}");
        assert!(msg.contains("missing required field"));
    }

    #[tokio::test]
    async fn mcp_wrap_coverage_normalizer_and_report_writer_emit_v1_report() {
        let dir = tempdir().unwrap();
        let normalized = dir.path().join("normalized.jsonl");
        let coverage_out = dir.path().join("coverage.json");

        normalize_decision_jsonl_to_coverage_jsonl(
            &fixture_path("decision_event_basic.jsonl"),
            &normalized,
        )
        .await
        .expect("normalization should succeed");

        let exit = coverage::write_generated_coverage_report(
            &normalized,
            &coverage_out,
            &["assay_policy_decide".to_string()],
            "decision_jsonl",
        )
        .await
        .expect("coverage generation should complete");
        assert_eq!(exit, crate::exit_codes::EXIT_SUCCESS);

        let report: Value =
            serde_json::from_str(&std::fs::read_to_string(&coverage_out).unwrap()).unwrap();
        assert_eq!(report["schema_version"], "coverage_report_v1");
        assert_eq!(report["run"]["source"], "decision_jsonl");
        assert_eq!(
            report["tools"]["tools_seen"],
            json!(["assay_policy_decide"])
        );
        assert_eq!(
            report["taxonomy"]["tool_classes_seen"],
            json!(["sink:network"])
        );
    }

    #[test]
    fn mcp_wrap_coverage_collect_declared_tools_ignores_wildcards() {
        let mut policy = McpPolicy::default();
        policy.tools.allow = Some(vec!["assay_policy_decide".into(), "*".into()]);
        policy.tools.deny = Some(vec!["assay_check_args".into(), "exec*".into()]);
        policy.schemas.insert("$defs".into(), json!({}));
        policy
            .schemas
            .insert("assay_check_sequence".into(), json!({"type": "object"}));

        let declared = collect_declared_tools(&policy);
        assert_eq!(
            declared,
            vec![
                "assay_check_args".to_string(),
                "assay_check_sequence".to_string(),
                "assay_policy_decide".to_string(),
            ]
        );
    }
}
