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

    #[cfg(target_os = "linux")]
    let enforcer_opt = if actual_enforcement {
        Some(crate::backend::prepare_landlock(policy, tmp_dir)?)
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

    let mut child = cmd
        .spawn()
        .map_err(|e| anyhow::anyhow!("failed to spawn child: {}", e))?;

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
