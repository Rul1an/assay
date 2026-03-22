use crate::backend::BackendType;
use crate::cli::args::SandboxArgs;
use crate::env_filter::EnvFilter;
use crate::exit_codes;
use crate::metrics;
use assay_evidence::types::{
    PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode,
    SandboxDegradationReasonCode,
};
use std::process::Stdio;
use tokio::time::Duration;

use crate::profile::{events::ProfileEvent, ProfileCollector, ProfileConfig};

pub async fn run(args: SandboxArgs) -> anyhow::Result<i32> {
    let mut profiler: Option<ProfileCollector> = None;
    let mut deferred_profile_events: Vec<ProfileEvent> = Vec::new();
    eprintln!("Assay Sandbox v0.1");
    eprintln!("──────────────────");

    // ABI Probing & Backend Selection
    let (probed_backend, caps) = crate::backend::detect_backend();

    // PR8: Enforcement Contract logic
    // Default: use Landlock if available, unless --dry-run is set.
    // If --enforce is explicitly set, we MUST use enforcement or fail (if fail_closed).
    let backend = probed_backend;
    let mut active_enforcement = matches!(backend, BackendType::Landlock) && !args.dry_run;

    if args.dry_run {
        active_enforcement = false;
        // Forced to NoopAudit for dry-run if we want to trace without blocking
        // (Assuming NoopAudit provides enough tracing for exit 4)
    }

    if args.enforce && !matches!(backend, BackendType::Landlock) {
        if args.fail_closed {
            eprintln!("ERROR: Active enforcement requested (--enforce) but no containment backend available.");
            return Ok(exit_codes::POLICY_UNENFORCEABLE);
        }
        deferred_profile_events.push(ProfileEvent::AuditFallback {
            reason: "landlock backend unavailable (degraded to audit)".to_string(),
            detail: None,
        });
        if let Some(payload) = backend_unavailable_degradation(&args, &backend) {
            deferred_profile_events.push(ProfileEvent::SandboxDegraded { payload });
        }
        eprintln!(
            "WARN: Active enforcement requested but not supported. Falling back to Audit mode."
        );
    }

    if !args.quiet {
        eprintln!(
            "Backend: {} (Mode: {}, FS:{}, NET:{}, ABI:v{})",
            backend.name(),
            if active_enforcement {
                "Containment"
            } else if args.dry_run {
                "Dry-Run"
            } else {
                "Audit"
            },
            if active_enforcement && caps.enforce_fs {
                "enforce"
            } else {
                "audit"
            },
            if active_enforcement && caps.enforce_net {
                "enforce"
            } else {
                "audit"
            },
            caps.abi_version
        );
    }

    // PR5.1 / PR6: Environment filtering
    let env_filter = build_env_filter(&args);
    let env_result = env_filter.filter_current();

    // Banner output
    if !args.quiet {
        eprintln!("  Env: {}", env_filter.format_banner(&env_result));

        if args.verbose {
            // Verbose: Show detailed scrubbing info
            eprintln!("  Env Details:");
            eprintln!("    Passed: {} vars", env_result.passed_count);
            if !env_result.scrubbed_keys.is_empty() {
                eprintln!("    Scrubbed: {:?}", env_result.scrubbed_keys);
            }
            if !env_result.exec_influence_stripped.is_empty() {
                eprintln!("    Stripped: {:?}", env_result.exec_influence_stripped);
            }
        }

        // Warn about exec-influence allows
        for var in &env_result.exec_influence_allowed {
            eprintln!("  ⚠ Exec-influence var ALLOWED: {}", var);
        }
    }

    // Metrics: record env filtering stats
    metrics::add(
        "env_scrubbed_keys_total",
        env_result.scrubbed_keys.len() as u64,
    );
    metrics::add(
        "env_exec_influence_stripped",
        env_result.exec_influence_stripped.len() as u64,
    );
    if !env_result.exec_influence_allowed.is_empty() {
        metrics::add(
            "env_exec_influence_allowed",
            env_result.exec_influence_allowed.len() as u64,
        );
    }

    // PR2: Load policy from file or use default MCP pack
    let policy = if let Some(ref path) = args.policy {
        match crate::policy::Policy::load(path) {
            Ok(p) => {
                if !args.quiet {
                    eprintln!("Policy:  {} (loaded)", path.display());
                }
                p
            }
            Err(e) => {
                if let Some(p) = &profiler {
                    p.note(format!("failed to load policy: {}", e));
                }
                eprintln!("WARN: Failed to load policy: {}. Using default.", e);
                crate::policy::mcp_server_minimal()
            }
        }
    } else {
        if !args.quiet {
            eprintln!("Policy:  mcp-server-minimal (default)");
        }
        crate::policy::mcp_server_minimal()
    };

    if !args.quiet {
        let (fs_allow, fs_deny, net_allow, net_deny) = policy.rule_counts();
        eprintln!(
            "Rules:   FS(allow:{} deny:{}) NET(allow:{} deny:{})",
            fs_allow, fs_deny, net_allow, net_deny
        );
        eprintln!("Command: {:?}", args.command);
        eprintln!("PID:     {}", std::process::id());
        if let Some(wd) = &args.workdir {
            eprintln!("Workdir: {}", wd.display());
        }
    }

    // PR1: Ensure trace directory exists
    // match crate::fs::ensure_assay_trace_dir() { ... } // (Optional logging)

    // PR5.4: Scoped /tmp with proper isolation
    let tmp_dir = create_scoped_tmp()?;

    // PR7: Initialize profiler if requested (passing tmp_dir for generalization)
    profiler = maybe_profile_begin(&args, Some(&tmp_dir));
    if let Some(p) = &profiler {
        for event in deferred_profile_events.drain(..) {
            p.record(event);
        }
    }

    if !args.quiet {
        eprintln!("Tmp:     {}", tmp_dir.display());
        eprintln!("──────────────────");
    }

    // Determine working directory
    let cwd = args.workdir.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    // Spawn child with sandbox isolation
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

    // PR7: record generalized argv0 if profiling
    if let Some(p) = &profiler {
        let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);

        // Resolve cmd_name via PATH if possible (deterministic resolution)
        let resolved_cmd = if std::path::Path::new(cmd_name).is_absolute() {
            std::path::PathBuf::from(cmd_name)
        } else {
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
        };

        let g = crate::profile::generalize::generalize_path(
            &resolved_cmd,
            &cwd,
            home.as_deref(),
            Some(&tmp_dir),
        );
        p.record(ProfileEvent::ExecObserved { argv0: g.rendered });
    }

    // Check Landlock compatibility before start
    let compat = crate::landlock_check::check_compatibility(&policy, &cwd, &tmp_dir);

    #[allow(unused_assignments, unused_mut)]
    let mut actual_enforcement = active_enforcement;

    if actual_enforcement && !compat.is_compatible() {
        if args.fail_closed {
            if let Some(p) = &profiler {
                p.record(ProfileEvent::EnforcementFailed {
                    reason: "landlock policy conflict (fail-closed)".to_string(),
                    detail: None,
                });
            }
            eprintln!("ERROR: Policy cannot be fully enforced (--fail-closed active)");
            eprintln!("E_POLICY_CONFLICT_DENY_WINS_UNENFORCEABLE");
            eprintln!(
                "       {} deny rule(s) conflict with allow rules",
                compat.conflicts.len()
            );
            metrics::increment("policy_conflict_fail_closed");
            return Ok(exit_codes::POLICY_UNENFORCEABLE);
        }

        if let Some(p) = &profiler {
            p.record(ProfileEvent::AuditFallback {
                reason: "landlock policy conflict (degraded to audit)".to_string(),
                detail: None,
            });
            if let Some(payload) = policy_conflict_degradation(&args, actual_enforcement, &compat) {
                p.record(ProfileEvent::SandboxDegraded { payload });
            }
        }

        eprintln!("WARN: Landlock policy conflict detected (Deny rule inside Allow root).");
        eprintln!(
            "WARN: Degrading to Audit mode (no containment). use --fail-closed to make this fatal."
        );
        metrics::increment("degraded_to_audit_conflict");
        actual_enforcement = false;
    }

    #[cfg(not(target_os = "linux"))]
    let _ = actual_enforcement;

    let mut cmd = tokio::process::Command::new(cmd_name);

    cmd.args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .current_dir(&cwd);

    // PR5.1: Apply filtered environment
    cmd.env_clear();
    for (key, value) in &env_result.filtered_env {
        cmd.env(key, value);
        if let Some(p) = &profiler {
            p.record(ProfileEvent::EnvProvidedKeys {
                key: key.clone(),
                scrubbed: false, // TODO(sandbox-scrub): set true when partial scrubbing exists
            });
        }
    }
    // Always set TMPDIR/TMP/TEMP to scoped tmp
    cmd.env("TMPDIR", &tmp_dir);
    cmd.env("TMP", &tmp_dir);
    cmd.env("TEMP", &tmp_dir);

    // Prepare Landlock ruleset in parent process (Safe allocation/IO)
    #[cfg(target_os = "linux")]
    let enforcer_opt = if actual_enforcement {
        // Note: prepare_landlock is blocking (file IO/parsing), but that's fine here before spawn
        Some(crate::backend::prepare_landlock(&policy, &tmp_dir)?)
    } else {
        None
    };

    // Apply Landlock isolation via pre_exec (child-side, before exec)
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

    // Handle timeout
    let status_res = if let Some(sec) = args.timeout {
        match tokio::time::timeout(Duration::from_secs(sec), child.wait()).await {
            Ok(res) => res,
            Err(_) => {
                // Timeout elapsed, kill process
                let _ = child.start_kill(); // ignore error if already exited
                let _ = child.wait().await; // clean up zombie
                eprintln!("\nTIMEOUT: Process exceeded {}s limit", sec);
                metrics::increment("sandbox_timeout");
                // Return generic failure or strict timeout code
                return Ok(exit_codes::COMMAND_FAILED);
            }
        }
    } else {
        child.wait().await
    };

    let status = status_res?;

    // PR7: Test Hook for Injected Events
    // Only enabled if test cfg OR explicit env var is set
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
        // PR8: Dry-run violation detection - Finish only once!
        let report = p.finish();
        let suggestions = report.to_suggestion(crate::profile::suggest::SuggestConfig {
            widen_dirs_to_glob: false,
        });

        if args.dry_run {
            let mut violations = 0;
            // Heuristic: if suggestions contain FS paths not in original policy, it's a violation
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
                maybe_profile_finish(report, &args)?;
                return Ok(exit_codes::WOULD_BLOCK);
            }
        }

        maybe_profile_finish(report, &args)?;
    }

    match status.code() {
        Some(code) => Ok(code),
        None => {
            eprintln!("sandbox error: child terminated by signal");
            Ok(exit_codes::INTERNAL_ERROR)
        }
    }
}

/// Build the environment filter based on CLI args.
fn build_env_filter(args: &SandboxArgs) -> EnvFilter {
    if args.env_passthrough {
        return EnvFilter::passthrough();
    }

    let mut filter = if args.env_strict {
        EnvFilter::strict()
    } else {
        EnvFilter::default() // scrub default
    };

    if args.env_strip_exec {
        filter = filter.with_strip_exec(true);
    }

    if let Some(ref allowed) = args.env_allow {
        filter = filter.with_allowed(allowed);
    }

    if args.env_safe_path {
        filter = filter.with_safe_path(true);
    }

    filter
}

/// Create a scoped temporary directory for sandbox isolation.
fn create_scoped_tmp() -> anyhow::Result<std::path::PathBuf> {
    let pid = std::process::id();

    // Get UID - use libc on Unix, fallback to USER env on other platforms
    #[cfg(unix)]
    let uid = unsafe { libc::getuid() };
    #[cfg(not(unix))]
    let uid = std::env::var("USER")
        .map(|u| u.chars().take(8).collect::<String>())
        .unwrap_or_else(|_| "sandbox".to_string());

    // Prefer XDG_RUNTIME_DIR (often tmpfs, more secure)
    // Falls back to /tmp
    let base = std::env::var("XDG_RUNTIME_DIR")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|_| std::path::PathBuf::from("/tmp"));

    let tmp_dir = base.join(format!("assay-{}-{}", uid, pid));

    // Create with restricted permissions
    std::fs::create_dir_all(&tmp_dir)?;

    // Set 0700 permissions (owner-only)
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = std::fs::Permissions::from_mode(0o700);
        std::fs::set_permissions(&tmp_dir, perms)?;
    }

    Ok(tmp_dir)
}

fn maybe_profile_begin(
    args: &SandboxArgs,
    assay_tmp: Option<&std::path::Path>,
) -> Option<ProfileCollector> {
    let _ = args.profile.as_ref()?; // If none, return early

    let cwd = std::env::current_dir()
        .ok()
        .unwrap_or_else(|| std::path::PathBuf::from("."));
    let home = std::env::var("HOME").ok().map(std::path::PathBuf::from);

    Some(ProfileCollector::new(ProfileConfig {
        cwd,
        home,
        assay_tmp: assay_tmp.map(|p| p.to_path_buf()),
    }))
}

fn maybe_profile_finish(
    report: crate::profile::ProfileReport,
    args: &SandboxArgs,
) -> anyhow::Result<()> {
    // Default SuggestConfig: widen dirs to glob by default for SOTA DX
    let sugg_cfg = crate::profile::suggest::SuggestConfig {
        widen_dirs_to_glob: true,
    };
    let suggestion = report.to_suggestion(sugg_cfg);

    let content = match args.profile_format.as_str() {
        "json" => crate::profile::writer::write_json(&suggestion)?,
        _ => crate::profile::writer::write_yaml(&suggestion),
    };

    let out_path = args.profile.as_ref().expect("profiler active");

    // Atomic Write
    crate::profile::writer::save_atomic(out_path, &content)?;

    let evidence_profile_path = evidence_profile_path(out_path, &args.profile_format);
    let run_id = evidence_profile_run_id(args, &report);
    let evidence_profile_name = out_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("assay-sandbox")
        .to_string();
    let evidence_profile = report.to_evidence_profile(&evidence_profile_name, &run_id);
    crate::cli::commands::profile_types::save_profile(&evidence_profile, &evidence_profile_path)?;

    // Optional report
    let report_path = args.profile_report.clone().unwrap_or_else(|| {
        let mut p = out_path.clone();
        if let Some(fname) = p.file_name() {
            let new_name = format!("{}.report.md", fname.to_string_lossy());
            p.set_file_name(new_name);
        } else {
            p.set_extension("report.md");
        }
        p
    });

    // Simple report generation
    let report_md = format!(
        "# Assay Profile Report\n\n\
         - **Command**: {:?}\n\
         - **Status**: Finished\n\
         - **Counters**: {:?}\n\
         - **Notes**: {:?}\n",
        args.command, suggestion.meta.counters, suggestion.meta.notes
    );

    // Atomic write report too
    crate::profile::writer::save_atomic(&report_path, &report_md)?;

    if !args.quiet {
        eprintln!(
            "Profile: {} (and {})",
            out_path.display(),
            report_path.display()
        );
        eprintln!("Evidence Profile: {}", evidence_profile_path.display());
    }

    Ok(())
}

fn backend_unavailable_degradation(
    args: &SandboxArgs,
    backend: &BackendType,
) -> Option<PayloadSandboxDegraded> {
    if !args.enforce || args.fail_closed || matches!(backend, BackendType::Landlock) {
        return None;
    }

    Some(PayloadSandboxDegraded {
        reason_code: SandboxDegradationReasonCode::BackendUnavailable,
        degradation_mode: SandboxDegradationMode::AuditFallback,
        component: SandboxDegradationComponent::Landlock,
        detail: None,
    })
}

fn policy_conflict_degradation(
    args: &SandboxArgs,
    actual_enforcement: bool,
    compat: &crate::landlock_check::LandlockCompatReport,
) -> Option<PayloadSandboxDegraded> {
    if !args.enforce || args.fail_closed || !actual_enforcement || compat.is_compatible() {
        return None;
    }

    Some(PayloadSandboxDegraded {
        reason_code: SandboxDegradationReasonCode::PolicyConflict,
        degradation_mode: SandboxDegradationMode::AuditFallback,
        component: SandboxDegradationComponent::Landlock,
        detail: None,
    })
}

fn evidence_profile_path(out_path: &std::path::Path, profile_format: &str) -> std::path::PathBuf {
    let ext = if profile_format == "json" {
        "evidence.json"
    } else {
        "evidence.yaml"
    };
    let mut path = out_path.to_path_buf();
    let stem = out_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("assay-sandbox");
    path.set_file_name(format!("{stem}.{ext}"));
    path
}

fn evidence_profile_run_id(args: &SandboxArgs, report: &crate::profile::ProfileReport) -> String {
    use sha2::Digest;

    let mut hasher = sha2::Sha256::new();
    hasher.update(args.command.join("\0").as_bytes());
    for (name, count) in &report.agg.counters {
        hasher.update(name.as_bytes());
        hasher.update(count.to_string().as_bytes());
    }
    for note in &report.agg.notes {
        hasher.update(note.as_bytes());
    }
    for (argv0, hits) in &report.agg.execs {
        hasher.update(argv0.as_bytes());
        hasher.update(hits.to_string().as_bytes());
    }
    let mut fs_entries = report.agg.fs.clone();
    fs_entries.sort();
    for (op, path, backend) in fs_entries {
        hasher.update(op.as_str().as_bytes());
        hasher.update(path.as_bytes());
        hasher.update(backend.as_str().as_bytes());
    }
    let mut degradations = report.agg.sandbox_degradations.clone();
    degradations.sort();
    for degradation in degradations {
        hasher.update(
            serde_json::to_string(&degradation)
                .expect("sandbox degradation payload should serialize deterministically")
                .as_bytes(),
        );
    }

    let digest = hex::encode(hasher.finalize());
    format!("sandbox_{}", &digest[..16])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sandbox_args() -> SandboxArgs {
        SandboxArgs {
            command: vec!["true".into()],
            policy: None,
            workdir: None,
            timeout: None,
            enforce: true,
            dry_run: false,
            fail_closed: false,
            env_strict: false,
            env_strip_exec: false,
            env_allow: None,
            env_passthrough: false,
            env_safe_path: false,
            profile: None,
            profile_format: "yaml".into(),
            profile_report: None,
            verbose: false,
            quiet: true,
        }
    }

    #[test]
    fn backend_unavailable_emits_degradation_when_enforcement_requested_and_run_continues() {
        let args = sandbox_args();
        let payload = backend_unavailable_degradation(&args, &BackendType::NoopAudit)
            .expect("expected degradation payload");
        assert_eq!(
            payload.reason_code,
            SandboxDegradationReasonCode::BackendUnavailable
        );
        assert_eq!(payload.detail, None);
    }

    #[test]
    fn intentional_permissive_mode_does_not_emit_degradation() {
        let mut args = sandbox_args();
        args.enforce = false;
        assert!(backend_unavailable_degradation(&args, &BackendType::NoopAudit).is_none());
    }

    #[test]
    fn fail_closed_backend_unavailable_does_not_emit_degradation() {
        let mut args = sandbox_args();
        args.fail_closed = true;
        assert!(backend_unavailable_degradation(&args, &BackendType::NoopAudit).is_none());
    }

    #[test]
    fn policy_conflict_emits_degradation_only_when_execution_continues() {
        let args = sandbox_args();
        let compat = crate::landlock_check::LandlockCompatReport {
            allowed_roots: Vec::new(),
            conflicts: vec![(
                std::path::PathBuf::from("/allow"),
                std::path::PathBuf::from("/allow/deny"),
            )],
        };
        let payload = policy_conflict_degradation(&args, true, &compat)
            .expect("expected degradation payload");
        assert_eq!(
            payload.reason_code,
            SandboxDegradationReasonCode::PolicyConflict
        );
    }

    #[test]
    fn fail_closed_policy_conflict_does_not_emit_degradation() {
        let mut args = sandbox_args();
        args.fail_closed = true;
        let compat = crate::landlock_check::LandlockCompatReport {
            allowed_roots: Vec::new(),
            conflicts: vec![(
                std::path::PathBuf::from("/allow"),
                std::path::PathBuf::from("/allow/deny"),
            )],
        };
        assert!(policy_conflict_degradation(&args, true, &compat).is_none());
    }

    #[test]
    fn evidence_profile_run_id_is_stable_across_equivalent_orderings() {
        use crate::profile::events::{BackendHint, FsOp};
        use crate::profile::{ProfileAgg, ProfileConfig, ProfileReport};
        use std::collections::BTreeMap;
        use std::path::PathBuf;

        let args = sandbox_args();
        let cfg = ProfileConfig {
            cwd: PathBuf::from("/repo"),
            home: None,
            assay_tmp: None,
        };

        let report_a = ProfileReport {
            version: 1,
            config: cfg.clone(),
            agg: ProfileAgg {
                counters: BTreeMap::new(),
                env_provided: BTreeMap::new(),
                execs: BTreeMap::from([(String::from("/usr/bin/true"), 1)]),
                fs: vec![
                    (FsOp::Write, "/tmp/b".into(), BackendHint::Landlock),
                    (FsOp::Read, "/tmp/a".into(), BackendHint::Injected),
                ],
                notes: vec!["audit_fallback: landlock policy conflict".into()],
                sandbox_degradations: vec![
                    PayloadSandboxDegraded {
                        reason_code: SandboxDegradationReasonCode::PolicyConflict,
                        degradation_mode: SandboxDegradationMode::AuditFallback,
                        component: SandboxDegradationComponent::Landlock,
                        detail: None,
                    },
                    PayloadSandboxDegraded {
                        reason_code: SandboxDegradationReasonCode::BackendUnavailable,
                        degradation_mode: SandboxDegradationMode::AuditFallback,
                        component: SandboxDegradationComponent::Landlock,
                        detail: Some("safe-context".into()),
                    },
                ],
            },
        };

        let report_b = ProfileReport {
            version: 1,
            config: cfg,
            agg: ProfileAgg {
                counters: BTreeMap::new(),
                env_provided: BTreeMap::new(),
                execs: BTreeMap::from([(String::from("/usr/bin/true"), 1)]),
                fs: vec![
                    (FsOp::Read, "/tmp/a".into(), BackendHint::Injected),
                    (FsOp::Write, "/tmp/b".into(), BackendHint::Landlock),
                ],
                notes: vec!["audit_fallback: landlock policy conflict".into()],
                sandbox_degradations: vec![
                    PayloadSandboxDegraded {
                        reason_code: SandboxDegradationReasonCode::BackendUnavailable,
                        degradation_mode: SandboxDegradationMode::AuditFallback,
                        component: SandboxDegradationComponent::Landlock,
                        detail: Some("safe-context".into()),
                    },
                    PayloadSandboxDegraded {
                        reason_code: SandboxDegradationReasonCode::PolicyConflict,
                        degradation_mode: SandboxDegradationMode::AuditFallback,
                        component: SandboxDegradationComponent::Landlock,
                        detail: None,
                    },
                ],
            },
        };

        assert_eq!(
            evidence_profile_run_id(&args, &report_a),
            evidence_profile_run_id(&args, &report_b)
        );
    }
}
