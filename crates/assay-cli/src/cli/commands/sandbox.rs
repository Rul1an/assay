use crate::backend::BackendType;
use crate::cli::args::SandboxArgs;
use crate::exit_codes;
use crate::metrics;

use crate::profile::{events::ProfileEvent, ProfileCollector};

mod bundle;
mod child;
mod degradation;
mod env;
mod otel;
mod profile;
mod tmp;

use child::run_child;
use degradation::{backend_unavailable_degradation, policy_conflict_degradation};
use env::build_env_filter;
use profile::maybe_profile_begin;
use tmp::create_scoped_tmp;

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

    // Determine working directory before profile initialization so suggestions
    // are generalized relative to the same cwd used by the child process.
    let cwd = args.workdir.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    // PR5.4: Scoped /tmp with proper isolation
    let tmp_dir = create_scoped_tmp()?;

    // PR7: Initialize profiler if requested (passing tmp_dir for generalization)
    profiler = maybe_profile_begin(&args, &cwd, Some(tmp_dir.path()));
    if let Some(p) = &profiler {
        for event in deferred_profile_events.drain(..) {
            p.record(event);
        }
    }

    if !args.quiet {
        eprintln!("Tmp:     {}", tmp_dir.path().display());
        eprintln!("──────────────────");
    }

    // Check Landlock compatibility before start
    let compat = crate::landlock_check::check_compatibility(&policy, &cwd, tmp_dir.path());

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

    run_child(
        &args,
        &policy,
        &env_result,
        tmp_dir.path(),
        &cwd,
        profiler,
        actual_enforcement,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::types::{
        PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode,
        SandboxDegradationReasonCode,
    };
    use profile::evidence_profile_run_id;

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
            bundle: None,
            otel_jsonl: None,
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
    fn profile_begin_uses_child_workdir_as_profile_cwd() {
        let mut args = sandbox_args();
        args.profile = Some(std::path::PathBuf::from("sandbox-profile.yaml"));
        let cwd = std::path::PathBuf::from("/repo/subdir");

        let report = profile::maybe_profile_begin(&args, &cwd, None)
            .expect("profile should start")
            .finish();

        assert_eq!(report.config.cwd, cwd);
    }

    #[test]
    fn scoped_tmp_dirs_are_unique_and_cleaned_on_drop() {
        let first = tmp::create_scoped_tmp().expect("first tmp dir");
        let second = tmp::create_scoped_tmp().expect("second tmp dir");
        let first_path = first.path().to_path_buf();
        let second_path = second.path().to_path_buf();

        assert_ne!(first_path, second_path);
        assert!(first_path.exists());
        assert!(second_path.exists());

        drop(first);
        assert!(!first_path.exists());
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
