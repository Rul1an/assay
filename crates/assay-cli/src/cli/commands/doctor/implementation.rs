use std::path::{Path, PathBuf};

use assay_core::config::{load_config_with_cause, path_resolver::PathResolver, LoadOptions};
use assay_core::errors::ConfigLoadError;

use crate::cli::args::common::OutputFormat;
use crate::cli::args::DoctorArgs;
use crate::cli::helpers::decide_exit;
use crate::diagnostics;
use crate::diagnostics::format::format_text;
use crate::exit_codes::{reason_for_unloadable_explicit_config, ReasonCode, RunOutcome};

use super::fixes::run_doctor_fix;
use super::parse_error::try_fix_parse_error;

/// The one decision for an explicit config that could not be loaded.
///
/// The reason identity, the recovery step and the exit class all come from the reason-code
/// registry, so the JSON report cannot disagree with the text path, and neither can disagree
/// with `assay run` on the same file. Before this existed, the JSON report named the reason
/// with a literal and both paths returned the test-failure class for a config error.
///
/// `fixes.rs` reads it too, for the config it re-loads after applying repairs. That return was a
/// literal `1` until independent review found it, which made the class of an unloadable config
/// depend on whether `--fix` had already written something.
pub(super) fn config_failure(path: &Path, err: &ConfigLoadError) -> RunOutcome {
    let path = path.display().to_string();
    RunOutcome::from_reason(
        reason_for_unloadable_explicit_config(err),
        Some(err.to_string()),
        Some(path.as_str()),
    )
}

/// Why no config was read when none was requested and none was found. Rendered by both channels,
/// from here, so the machine report cannot say something different from the line a human reads.
const NO_CONFIG_FOUND: &str = "No config found; run inside project or use --config";

/// Whether the config check ran, for a consumer that reads only the JSON document.
///
/// Three states, one always-present key. The text channel has always distinguished them —
/// `Policy Check:` against `Policy Check: SKIPPED` against `Config Status: FAILED` — while the
/// machine channel expressed a skipped check by omitting `data_diagnostics`, which is the same
/// shape as a check that ran and found nothing to a consumer that does not know the difference.
/// Publishing the state positively is what lets the reading instruction on
/// `data_diagnostics[].severity` be safe: absence of findings is only a clean config when
/// `status` is `checked`.
///
/// The key is present in every JSON report rather than only in the skipped one, so the question
/// "was this config read?" has an answer a consumer can key on without knowing which absences
/// mean what. `reason` accompanies the two states that produced no diagnostics; on `failed` it
/// restates `config_error.message` from the same value rather than deriving a second one, because
/// the registered diagnosis stays the carrier a consumer branches on.
fn config_check_skipped(reason: &str) -> serde_json::Value {
    serde_json::json!({
        "status": "skipped",
        "reason": reason,
    })
}

fn config_check_marker(checked: bool, error: Option<&str>) -> serde_json::Value {
    match (checked, error) {
        (_, Some(message)) => serde_json::json!({
            "status": "failed",
            "reason": message,
        }),
        (true, None) => serde_json::json!({ "status": "checked" }),
        (false, None) => config_check_skipped(NO_CONFIG_FOUND),
    }
}

fn insert_outcome_fields(
    obj: &mut serde_json::Map<String, serde_json::Value>,
    outcome: &RunOutcome,
) {
    obj.insert(
        "reason_code".to_string(),
        serde_json::json!(outcome.reason_code),
    );
    obj.insert(
        "next_step".to_string(),
        serde_json::json!(outcome.next_step),
    );
}

/// The one decision for a combination this command refuses itself.
///
/// Both channels read this outcome. The text path keeps the existing rejection line on stderr;
/// the JSON path publishes the same identity on `assay.doctor_report.v0`. The class comes from
/// `EInvalidArgs`, so a clap refusal and a command refusal of the same category no longer
/// disagree about the integer.
fn invalid_argument_outcome(args: &DoctorArgs) -> Option<RunOutcome> {
    let message = if args.fix && args.format == OutputFormat::Json {
        "doctor --fix currently supports text output only; use --format text"
    } else if (args.yes || args.dry_run) && !args.fix {
        "doctor: --yes/--dry-run require --fix"
    } else {
        return None;
    };
    Some(RunOutcome::from_reason(
        ReasonCode::EInvalidArgs,
        Some(message.to_string()),
        None,
    ))
}

fn reject_invalid_args(format: OutputFormat, outcome: RunOutcome) -> anyhow::Result<i32> {
    if format == OutputFormat::Json {
        let report = diagnostics::probe_system();
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut json_out = serde_json::to_value(&report)?;
        if let Some(obj) = json_out.as_object_mut() {
            obj.insert("generated_at".to_string(), serde_json::json!(timestamp));
            let skip_reason = outcome
                .message
                .as_deref()
                .unwrap_or(outcome.reason_code.as_str());
            obj.insert(
                "config_check".to_string(),
                config_check_skipped(skip_reason),
            );
            insert_outcome_fields(obj, &outcome);
            if let Some(message) = &outcome.message {
                obj.insert("message".to_string(), serde_json::json!(message));
            }
        }
        println!("{}", serde_json::to_string_pretty(&json_out)?);
        return Ok(outcome.exit_code);
    }
    if let Some(message) = &outcome.message {
        eprintln!("{message}");
    }
    Ok(outcome.exit_code)
}

pub async fn run(args: DoctorArgs, legacy_mode: bool) -> anyhow::Result<i32> {
    if let Some(outcome) = invalid_argument_outcome(&args) {
        return reject_invalid_args(args.format, outcome);
    }

    // 1. Unified System Diagnostics
    let report = diagnostics::probe_system();

    // 2. Data/Config Diagnostics via Core
    let (target_path, explicit) = match args.config.clone() {
        Some(p) => (p, true),
        None => (PathBuf::from("eval.yaml"), false),
    };

    let (cfg, cfg_err) = if explicit || target_path.exists() {
        match load_config_with_cause(
            &target_path,
            LoadOptions {
                legacy_mode,
                ..Default::default()
            },
        ) {
            Ok(c) => (Some(c), None),
            Err(e) => (None, Some(e)),
        }
    } else {
        (None, None)
    };

    if args.format == OutputFormat::Json {
        let timestamp = chrono::Utc::now().to_rfc3339();
        let mut json_out = serde_json::to_value(&report)?;
        // Stays `0` when no config was examined: nothing was checked, so nothing failed. The
        // `config_check` marker is what tells a consumer which of the two a `0` means. When a
        // config was examined, `decide_exit` answers — the same function `validate` and `run` use,
        // so one diagnostic does not get three exit codes depending on which command met it.
        let mut exit = 0;

        if let Some(obj) = json_out.as_object_mut() {
            let err_text = cfg_err.as_ref().map(ToString::to_string);
            obj.insert("generated_at".to_string(), serde_json::json!(timestamp));
            obj.insert(
                "config_check".to_string(),
                config_check_marker(cfg.is_some(), err_text.as_deref()),
            );

            if let Some(err) = &cfg_err {
                let outcome = config_failure(&target_path, err);
                insert_outcome_fields(obj, &outcome);
                obj.insert(
                    "config_error".to_string(),
                    serde_json::json!({
                        "message": outcome.message,
                        "code": outcome.reason_code,
                    }),
                );
                println!("{}", serde_json::to_string_pretty(&json_out)?);
                return Ok(outcome.exit_code);
            }

            if let Some(c) = &cfg {
                let resolver = PathResolver::new(&target_path);
                let opts = assay_core::doctor::DoctorOptions {
                    config_path: target_path.clone(),
                    trace_file: args.trace_file.clone(),
                    baseline_file: args.baseline.clone(),
                    db_path: args.db.clone(),
                    replay_strict: args.replay_strict,
                };
                let core_report = assay_core::doctor::doctor(c, &opts, &resolver).await?;
                obj.insert(
                    "data_diagnostics".to_string(),
                    serde_json::to_value(&core_report.diagnostics)?,
                );
                obj.insert(
                    "data_suggestions".to_string(),
                    serde_json::to_value(&core_report.suggested_actions)?,
                );
                exit = decide_exit(&core_report.diagnostics);
            }
        }

        println!("{}", serde_json::to_string_pretty(&json_out)?);
        return Ok(exit);
    }

    // Text Format
    let text_output = format_text(&report);
    println!("{}", text_output);

    if let Some(e) = &cfg_err {
        println!("\nConfig Status: FAILED");
        println!("  File:     {}", target_path.display());
        println!("  Error:    {}\n", e);

        if args.fix {
            return try_fix_parse_error(&args, &target_path, &e.to_string(), legacy_mode);
        }
        return Ok(config_failure(&target_path, e).exit_code);
    }

    if let Some(c) = cfg {
        println!("\nPolicy Check:");
        println!("  Config:   {}", target_path.display());
        println!("  Suite:    {}", c.suite);

        let resolver = PathResolver::new(&target_path);
        let opts = assay_core::doctor::DoctorOptions {
            config_path: target_path.clone(),
            trace_file: args.trace_file.clone(),
            baseline_file: args.baseline.clone(),
            db_path: args.db.clone(),
            replay_strict: args.replay_strict,
        };
        let core_report = assay_core::doctor::doctor(&c, &opts, &resolver).await?;

        if !core_report.diagnostics.is_empty() {
            println!("  Issues:   {}", core_report.diagnostics.len());
            for d in &core_report.diagnostics {
                println!("    - [{}] [{}] {}", d.code, d.severity, d.message);
            }
        } else {
            println!("  Issues:   None (Clean)");
        }

        if args.fix {
            let fix_result =
                run_doctor_fix(&args, &target_path, &core_report.diagnostics, legacy_mode).await?;
            return Ok(fix_result);
        }

        return Ok(decide_exit(&core_report.diagnostics));
    }

    println!("\nPolicy Check: SKIPPED ({NO_CONFIG_FOUND})");
    Ok(0)
}
