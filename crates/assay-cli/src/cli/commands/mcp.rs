use super::super::args::{ManifestSub, McpArgs, McpSub, McpWrapArgs};
use super::coverage;
use super::session_state_window;
use anyhow::Context;
use assay_core::mcp::policy::{keys_wrap_does_not_act_on, McpPolicy};
use assay_core::mcp::proxy::{McpProxy, OutputCommit, ProxyConfig, ProxyConfigRaw};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

mod coverage_input;
mod preflight;
mod session_context;
mod session_stop;
mod tdt;

use coverage_input::{collect_declared_tools, normalize_decision_jsonl_to_coverage_jsonl};
use tdt::build_tdt_producer;

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
        McpSub::Preflight(preflight_args) => preflight::run(preflight_args),
        McpSub::Manifest(manifest_args) => {
            match manifest_args.cmd {
                ManifestSub::Candidate { from_observed, out } => {
                    assay_mcp_server::manifest_promotion::export_candidate(&from_observed, &out)?;
                }
                ManifestSub::Promote {
                    candidate,
                    source,
                    out,
                } => {
                    assay_mcp_server::manifest_promotion::promote(&candidate, &source, &out)?;
                }
            }
            Ok(0)
        }
    }
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
        let policy = McpPolicy::from_file(&args.policy)?;
        // Loaded above, so this read fails only if the file vanished in between.
        let file = std::fs::read(&args.policy).unwrap_or_default();
        for note in keys_wrap_does_not_act_on(&file) {
            eprintln!("[assay] WARNING: {note}");
        }
        policy
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
    if args.session_stop_out.is_some() && args.decision_log.is_some() && !args.decision_log_new {
        anyhow::bail!("--session-stop-out with --decision-log requires --decision-log-new");
    }
    let stop_source = args
        .session_stop_out
        .as_ref()
        .map(|_| {
            config
                .event_source
                .clone()
                .context("session stop requires event source")
        })
        .transpose()?;
    let context_source = args
        .session_context_out
        .as_ref()
        .map(|_| {
            config
                .event_source
                .clone()
                .context("session context requires event source")
        })
        .transpose()?;
    let one_run_outputs = session_context::OneRunOutputs {
        stop: args.session_stop_out.clone(),
        decision_log: args.decision_log.clone().filter(|_| args.decision_log_new),
        receipts: args.request_receipts_out.clone(),
        context: args.session_context_out.clone(),
        effective_decision_log: effective_decision_log.clone(),
        audit_log: args.audit_log.clone(),
        coverage: args.coverage_out.clone(),
        state_window: args.state_window_out.clone(),
        tool_decision_truth: args.tool_decision_truth_out.clone(),
    };
    // A refused path stops the start here, before any output producer opens a path or a child is
    // spawned. The outputs themselves are created at the first frame that is not a probe.
    one_run_outputs.preflight()?;
    if args.decision_log_new {
        // The new log comes from the commit below, not from a path the proxy would open itself.
        config.decision_log_path = None;
    }
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

    let stop_output = Arc::new(Mutex::new(None));
    let commit: OutputCommit = {
        let stop_output = stop_output.clone();
        let argv = args.command.clone();
        let server_id = state_window_server_id.clone();
        let session_id = session_id.clone();
        let dry_run = args.dry_run;
        Box::new(move || {
            let commit = || -> anyhow::Result<_> {
                let created = one_run_outputs.create()?;
                *stop_output.lock().unwrap_or_else(|e| e.into_inner()) = created.stop;
                if let (Some(output), Some(source)) = (created.context, &context_source) {
                    let event =
                        session_context::event(&argv, source, &server_id, dry_run, &session_id)?;
                    session_context::write(output, &event)
                        .context("failed to write session context")?;
                }
                Ok((created.decision_log, created.receipts))
            };
            commit().map_err(|error| std::io::Error::other(format!("{error:#}")))
        })
    };

    // Run (blocking for now, as it manages threads)
    let report =
        tokio::task::spawn_blocking(move || proxy.run_with_deferred_outputs(commit)).await?;
    let probe_only = report.client_probe_only;
    if probe_only {
        eprintln!("[assay] probe-only run (server/discover only): created no one-run outputs");
    }
    let stop_output = stop_output.lock().unwrap_or_else(|e| e.into_inner()).take();
    if let (Some(output), Some(source)) = (stop_output, stop_source) {
        let event = session_stop::event(
            &report,
            &source,
            &state_window_server_id,
            &session_id,
            args.session_context_out.is_some(),
            effective_decision_log.is_some(),
            temp_decision_log.is_some(),
        );
        session_context::write(output, &event).with_context(|| {
            format!(
                "failed to write requested session stop (child status: {:?})",
                report.child_status
            )
        })?;
    }
    let wrapped_code = report.result?;
    // A probe is not a session, so nothing reports on it.
    if probe_only {
        return Ok(wrapped_code);
    }

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
