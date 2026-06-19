use super::super::args::{McpArgs, McpSub, McpWrapArgs};
use super::coverage;
use super::session_state_window;
use anyhow::{Context, Result};
use assay_core::mcp::policy::McpPolicy;
use assay_core::mcp::proxy::{McpProxy, ProxyConfig, ProxyConfigRaw, TdtProducer};
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
        McpSub::Discover(discover_args) => super::discover::run(discover_args).await,
        McpSub::Inventory(inventory_args) => super::inventory::run(inventory_args).await,
        McpSub::Kill(kill_args) => super::kill::run(kill_args).await,
        McpSub::Tool(tool_args) => Ok(super::tool::cmd_tool(tool_args.cmd)),
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

/// EXPERIMENTAL: build the opt-in tool-decision-truth carrier producer from the environment, failing
/// closed if the key material is absent or malformed. The HMAC key is read once here, moved into the
/// producer (held in memory only), and removed from this process's environment so the wrapped child
/// server — spawned with the inherited environment — cannot read it. The key is never logged or written
/// to disk.
fn build_tdt_producer(out_path: PathBuf) -> anyhow::Result<TdtProducer> {
    const KEY_VAR: &str = "ASSAY_TDT_HMAC_KEY";
    const KEY_ID_VAR: &str = "ASSAY_TDT_HMAC_KEY_ID";

    let key = std::env::var(KEY_VAR).ok();
    let key_id = std::env::var(KEY_ID_VAR).ok();

    // Remove the key (and its id) from this process's environment immediately, before the proxy spawns
    // the wrapped child with an inherited environment, so the child cannot read the key and forge
    // carriers. The values were captured above; validation happens in the pure helper below.
    std::env::remove_var(KEY_VAR);
    std::env::remove_var(KEY_ID_VAR);

    tdt_producer_from_material(out_path, key, key_id)
}

/// Pure fail-closed validator: turn optional key material into a [`TdtProducer`], or a startup error
/// naming exactly what is missing or malformed. Split out from the environment read so the fail-closed
/// contract is testable without mutating process-global state.
fn tdt_producer_from_material(
    out_path: PathBuf,
    key: Option<String>,
    key_id: Option<String>,
) -> anyhow::Result<TdtProducer> {
    let key = key.ok_or_else(|| {
        anyhow::anyhow!(
            "--tool-decision-truth-out is set but ASSAY_TDT_HMAC_KEY is missing; the tool-decision-truth producer fails closed. Set the HMAC key in the environment (never on the command line)."
        )
    })?;
    let key_id = key_id.ok_or_else(|| {
        anyhow::anyhow!(
            "--tool-decision-truth-out is set but ASSAY_TDT_HMAC_KEY_ID is missing; the tool-decision-truth producer fails closed."
        )
    })?;
    if key.is_empty() {
        anyhow::bail!(
            "ASSAY_TDT_HMAC_KEY is empty; the tool-decision-truth producer fails closed."
        );
    }
    // key_id must match the digest-prefix charset the carrier binds (`[A-Za-z0-9._-]`, non-empty), so the
    // minted args_digest is well-formed and verifiable downstream. This mirrors the core `args_digest`
    // guard, but here it fails closed loudly at startup instead of silently dropping carriers per call.
    let key_id_ok = !key_id.is_empty()
        && key_id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'));
    if !key_id_ok {
        anyhow::bail!(
            "ASSAY_TDT_HMAC_KEY_ID is empty or malformed (allowed characters: A-Z a-z 0-9 . _ -); the tool-decision-truth producer fails closed."
        );
    }
    // Fail closed at startup when the opted-in sink cannot be opened. Otherwise a run could proceed with
    // `--tool-decision-truth-out` enabled while minting no carriers, which is a half-configured producer.
    std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .with_context(|| {
            format!(
                "tool-decision-truth sink is not writable at {}; the producer fails closed",
                out_path.display()
            )
        })?;
    Ok(TdtProducer::new(out_path, key.into_bytes(), key_id))
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
    let mut config = ProxyConfig::try_from_raw(raw)?;
    // EXPERIMENTAL opt-in: wire the tool-decision-truth carrier producer (separate append-only NDJSON
    // sink). Fails closed at startup if enabled but the env key material is missing or malformed.
    if let Some(out_path) = args.tool_decision_truth_out.clone() {
        config.tdt_producer = Some(build_tdt_producer(out_path)?);
    }
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

    // ── EXPERIMENTAL tool-decision-truth producer: fail-closed key handling ──────────────────────

    #[test]
    fn tdt_producer_fails_closed_when_key_missing() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            None,
            Some("kid-v0".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ASSAY_TDT_HMAC_KEY is missing"), "got: {err}");
    }

    #[test]
    fn tdt_producer_fails_closed_when_key_empty() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some(String::new()),
            Some("kid-v0".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("ASSAY_TDT_HMAC_KEY is empty"), "got: {err}");
    }

    #[test]
    fn tdt_producer_fails_closed_when_key_id_missing() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some("k".into()),
            None,
        )
        .unwrap_err()
        .to_string();
        assert!(
            err.contains("ASSAY_TDT_HMAC_KEY_ID is missing"),
            "got: {err}"
        );
    }

    #[test]
    fn tdt_producer_fails_closed_when_key_id_malformed() {
        let err = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some("k".into()),
            Some("bad:id".into()),
        )
        .unwrap_err()
        .to_string();
        assert!(err.contains("malformed"), "got: {err}");
    }

    #[test]
    fn tdt_producer_accepts_valid_material_and_debug_redacts_key() {
        let producer = tdt_producer_from_material(
            PathBuf::from("/tmp/carriers.ndjson"),
            Some("super-secret-key".into()),
            Some("kid-v0".into()),
        )
        .expect("valid material");
        let dbg = format!("{producer:?}");
        assert!(
            dbg.contains("<redacted>"),
            "Debug must redact the key: {dbg}"
        );
        assert!(
            !dbg.contains("super-secret-key"),
            "Debug must not leak the key: {dbg}"
        );
    }

    #[test]
    fn tdt_producer_fails_closed_when_sink_cannot_be_opened() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("missing-parent").join("carriers.ndjson");
        let err =
            tdt_producer_from_material(out, Some("super-secret-key".into()), Some("kid-v0".into()))
                .unwrap_err()
                .to_string();
        assert!(
            err.contains("tool-decision-truth sink is not writable"),
            "got: {err}"
        );
    }

    #[test]
    fn producer_carrier_line_roundtrips_import_verify_project() {
        use crate::cli::args::ProjectOtelArgs;
        use crate::cli::commands::evidence::tool_decision_truth::{
            cmd_tool_decision_truth, ToolDecisionTruthArgs,
        };
        use crate::cli::commands::evidence::verify_tool_decision_truth::{
            cmd_verify_tool_decision_truth, VerifyFormat, VerifyToolDecisionTruthArgs,
        };
        use crate::cli::commands::project_otel;
        use assay_core::mcp::policy::McpPolicy;
        use assay_core::mcp::tool_decision_truth as tdt;
        use assay_core::mcp::tool_decision_truth::DecisionEvidence;

        let policy: McpPolicy = serde_json::from_value(json!({
            "version": "1",
            "tools": {"allow": ["deploy"], "deny": ["delete_all"]},
            "schemas": {"deploy": {"type": "object", "required": ["env"],
                "properties": {"env": {"enum": ["staging", "prod"]}}}},
            "enforcement": {"unconstrained_tools": "warn"}
        }))
        .unwrap();

        // Build a carrier exactly as the live producer does (same builder + same producer arguments) and
        // write it as one NDJSON line, mirroring the producer sink.
        let carrier = tdt::build_classified_record(
            &policy,
            "deploy",
            &json!({"env": "prod", "trace": "ZZSENTINELRAWZZ"}),
            0,
            b"producer-test-key-v0",
            "fixture-kid-v0",
            "authoritative_boundary",
            "call-0",
            "ok",
            "present",
            &DecisionEvidence::default(),
        )
        .expect("carrier builds");

        let dir = tempdir().unwrap();
        let sink = dir.path().join("carriers.ndjson");
        std::fs::write(
            &sink,
            format!("{}\n", serde_json::to_string(&carrier).unwrap()),
        )
        .unwrap();

        // Extract one line; PR9a imports a single carrier JSON (multi-carrier import is out of scope).
        let body = std::fs::read_to_string(&sink).unwrap();
        let line = body.lines().next().unwrap();
        let carrier_json = dir.path().join("carrier.json");
        std::fs::write(&carrier_json, line).unwrap();

        let bundle = dir.path().join("tdt.tar.gz");
        let code = cmd_tool_decision_truth(ToolDecisionTruthArgs {
            carrier: carrier_json,
            bundle_out: bundle.clone(),
            run_id: "producer-roundtrip".to_string(),
            import_time: Some("2026-06-19T00:00:00Z".to_string()),
        })
        .expect("import runs");
        assert_eq!(code, 0, "import of a producer carrier line should succeed");

        let vcode = cmd_verify_tool_decision_truth(VerifyToolDecisionTruthArgs {
            bundle: bundle.clone(),
            format: VerifyFormat::Json,
        })
        .expect("verify runs");
        assert_eq!(
            vcode, 0,
            "verify should report ok for a producer-emitted carrier"
        );

        let proj = dir.path().join("projection.json");
        let pcode = project_otel::run(ProjectOtelArgs {
            capability_surface: None,
            evidence_bundle: Some(bundle),
            observation_health: None,
            enforcement_health: None,
            out: Some(proj.clone()),
        })
        .expect("project runs");
        assert_eq!(pcode, 0, "projection over verified evidence should succeed");
        let projection = std::fs::read_to_string(&proj).unwrap();
        assert!(
            projection.contains("assay.tdt."),
            "projection should carry tdt identity attributes: {projection}"
        );
        assert!(
            !projection.contains("ZZSENTINELRAWZZ"),
            "projection must not carry raw arguments"
        );
    }
}
