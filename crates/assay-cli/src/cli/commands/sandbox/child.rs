use super::profile::maybe_profile_finish;
use crate::cli::args::SandboxArgs;
use crate::env_filter::EnvFilterResult;
use crate::exit_codes;
use crate::metrics;
use crate::profile::{events::ProfileEvent, ProfileCollector};
use std::path::Path;
use std::process::Stdio;
use tokio::time::Duration;

pub(super) async fn run_child(
    args: &SandboxArgs,
    policy: &crate::policy::Policy,
    env_result: &EnvFilterResult,
    tmp_dir: &Path,
    cwd: &Path,
    profiler: Option<ProfileCollector>,
    actual_enforcement: bool,
) -> anyhow::Result<i32> {
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

    if let Some(p) = &profiler {
        let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);
        let resolved_cmd = resolve_command_path(cmd_name);
        let g = crate::profile::generalize::generalize_path(
            &resolved_cmd,
            cwd,
            home.as_deref(),
            Some(tmp_dir),
        );
        p.record(ProfileEvent::ExecObserved { argv0: g.rendered });
    }

    #[cfg(not(target_os = "linux"))]
    let _ = actual_enforcement;

    let mut cmd = tokio::process::Command::new(cmd_name);

    cmd.args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(cwd);

    cmd.env_clear();
    for (key, value) in &env_result.filtered_env {
        cmd.env(key, value);
        if let Some(p) = &profiler {
            p.record(ProfileEvent::EnvProvidedKeys {
                key: key.clone(),
                scrubbed: false,
            });
        }
    }
    cmd.env("TMPDIR", tmp_dir);
    cmd.env("TMP", tmp_dir);
    cmd.env("TEMP", tmp_dir);

    // Landlock-net enforcement plan. `Some(ports)` builds a combined FS+NET ruleset; a rejected
    // policy fails closed BEFORE spawn with a `failed` enforcement_health.v1 artifact.
    #[cfg(target_os = "linux")]
    let net_allow_ports: Option<Vec<u16>> = if actual_enforcement && args.enforce_net {
        let abi = crate::backend::detect_backend().1.abi_version;
        match crate::landlock_net::plan_landlock_net_ports(&policy.net) {
            Ok(ports) => Some(ports),
            Err(rejects) => {
                let reason = net_reject_to_reason_code(&rejects);
                let detail = rejects
                    .iter()
                    .map(|r| format!("{}: {}", r.reason.as_str(), r.entry))
                    .collect::<Vec<_>>()
                    .join("; ");
                let health = crate::enforcement_health_v1::EnforcementHealthV1::landlock_failed(
                    abi,
                    reason,
                    detail,
                    abi >= 4,
                    false,
                );
                write_enforcement_health_v1(args, &health)?;
                if !args.quiet {
                    eprintln!(
                        "ERROR: network policy is not Landlock-net enforceable (fail-closed)"
                    );
                }
                return Ok(exit_codes::WOULD_BLOCK);
            }
        }
    } else {
        None
    };

    #[cfg(target_os = "linux")]
    let enforcer_opt = if actual_enforcement {
        Some(crate::backend::prepare_landlock(
            policy,
            tmp_dir,
            net_allow_ports.as_deref(),
        )?)
    } else {
        None
    };

    #[cfg(all(target_os = "linux", target_family = "unix"))]
    {
        if let Some(mut enforcer) = enforcer_opt {
            unsafe {
                cmd.pre_exec(move || {
                    enforcer.enforce()?;
                    Ok(())
                });
            }
        }
    }

    // The child→parent ack is std's pre_exec error channel: if `enforce()` (no_new_privs +
    // restrict_self) returns an error in the child, the closure fails and `spawn()` returns that
    // error, so we never record `restrict_self_confirmed` on an unenforced child.
    let spawn_result = cmd.spawn();

    #[cfg(target_os = "linux")]
    if actual_enforcement && args.enforce_net {
        let abi = crate::backend::detect_backend().1.abi_version;
        match &spawn_result {
            Ok(_) => {
                let ports = net_allow_ports.clone().unwrap_or_default();
                let health = crate::enforcement_health_v1::EnforcementHealthV1::landlock_active(
                    abi, ports, None,
                );
                write_enforcement_health_v1(args, &health)?;
            }
            Err(_) => {
                let health = crate::enforcement_health_v1::EnforcementHealthV1::landlock_failed(
                    abi,
                    crate::enforcement_health_v1::ReasonCode::RestrictSelfFailed,
                    "landlock restrict_self failed in the enforcing child",
                    abi >= 4,
                    true,
                );
                write_enforcement_health_v1(args, &health)?;
            }
        }
    }

    let mut child = spawn_result.map_err(|e| anyhow::anyhow!("failed to spawn child: {}", e))?;

    let status_res = if let Some(sec) = args.timeout {
        match tokio::time::timeout(Duration::from_secs(sec), child.wait()).await {
            Ok(res) => res,
            Err(_) => {
                let _ = child.start_kill();
                let _ = child.wait().await;
                eprintln!("\nTIMEOUT: Process exceeded {}s limit", sec);
                metrics::increment("sandbox_timeout");
                return Ok(exit_codes::COMMAND_FAILED);
            }
        }
    } else {
        child.wait().await
    };

    let status = status_res?;

    #[cfg(any(test, feature = "profile-test-hook"))]
    if let Some(events) = crate::profile::events::try_load_test_events() {
        if let Some(p) = &profiler {
            p.note("injected_test_events: true");
            for ev in events {
                p.record(ev);
            }
        }
    }

    if let Some(p) = profiler {
        let report = p.finish();
        let suggestions = report.to_suggestion(crate::profile::suggest::SuggestConfig {
            widen_dirs_to_glob: false,
        });

        if args.dry_run {
            let mut violations = 0;
            for path in &suggestions.fs.allow {
                if !policy.fs.allow.iter().any(|p| p == path) {
                    violations += 1;
                    if !args.quiet {
                        eprintln!(
                            "DRY-RUN VIOLATION: Would have blocked FS access to: {}",
                            path
                        );
                    }
                }
            }

            if violations > 0 {
                if !args.quiet {
                    eprintln!("──────────────────");
                    eprintln!("DRY-RUN: Found {} potential violations.", violations);
                }
                maybe_profile_finish(report, args)?;
                return Ok(exit_codes::WOULD_BLOCK);
            }
        }

        maybe_profile_finish(report, args)?;
    }

    match status.code() {
        Some(code) => Ok(code),
        None => {
            eprintln!("sandbox error: child terminated by signal");
            Ok(exit_codes::INTERNAL_ERROR)
        }
    }
}

/// Write the `assay.enforcement_health.v1` artifact when `--enforcement-health` is set. Fail-closed:
/// a requested artifact that cannot be written is an error so the caller does not exit successfully
/// in a state where the evidence is absent on disk (the same rule v0 enforces).
#[cfg(target_os = "linux")]
fn write_enforcement_health_v1(
    args: &SandboxArgs,
    health: &crate::enforcement_health_v1::EnforcementHealthV1,
) -> anyhow::Result<()> {
    use anyhow::Context;
    if let Some(path) = args.enforcement_health.as_ref() {
        health.write_to(path).with_context(|| {
            format!(
                "failed to write enforcement_health.v1 to {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

/// All policy-not-expressible rejections collapse to a single reason code; the specific entries and
/// their per-entry reasons travel in the artifact's `detail` string.
#[cfg(target_os = "linux")]
fn net_reject_to_reason_code(
    _rejects: &[crate::landlock_net::NetReject],
) -> crate::enforcement_health_v1::ReasonCode {
    crate::enforcement_health_v1::ReasonCode::PolicyNotExpressible
}

fn resolve_command_path(cmd_name: &str) -> std::path::PathBuf {
    if std::path::Path::new(cmd_name).is_absolute() {
        return std::path::PathBuf::from(cmd_name);
    }

    std::env::var_os("PATH")
        .and_then(|paths| {
            std::env::split_paths(&paths).find_map(|dir| {
                let full_path = dir.join(cmd_name);
                if full_path.exists() {
                    Some(full_path)
                } else {
                    None
                }
            })
        })
        .unwrap_or_else(|| std::path::PathBuf::from(cmd_name))
}
