use crate::backend::BackendType;
use crate::cli::args::SandboxArgs;
use crate::env_filter::EnvFilter;
use crate::exit_codes;
use crate::metrics;
use std::process::Stdio;
use tokio::time::Duration;

pub async fn run(args: SandboxArgs) -> anyhow::Result<i32> {
    eprintln!("Assay Sandbox v0.1");
    eprintln!("──────────────────");

    // PR3: Detect and display backend
    let (backend, caps) = crate::backend::detect_backend();
    let mode = if caps.audit_only {
        "Audit"
    } else {
        "Containment"
    };
    eprintln!(
        "Backend: {} (Mode: {}, FS:{}, NET:{})",
        backend.name(),
        mode,
        if caps.enforce_fs { "enforce" } else { "audit" },
        if caps.enforce_net { "enforce" } else { "audit" }
    );

    // PR5.1 / PR6: Environment filtering
    let env_filter = build_env_filter(&args);
    let env_result = env_filter.filter_current();

    // Banner output
    if !args.quiet {
        eprintln!("  Env: {}", env_filter.format_banner(&env_result));

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
    if !args.quiet {
        eprintln!("Tmp:     {}", tmp_dir.display());
        eprintln!("──────────────────");
    }

    // Determine working directory
    let cwd = args.workdir.clone().unwrap_or_else(|| {
        std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
    });

    // Check Landlock compatibility before start
    let compat = crate::landlock_check::check_compatibility(&policy, &cwd, &tmp_dir);

    #[allow(unused_assignments, unused_mut)]
    let mut should_enforce = matches!(backend, BackendType::Landlock);

    if should_enforce && !compat.is_compatible() {
        if args.fail_closed {
            eprintln!("ERROR: Policy cannot be fully enforced (--fail-closed active)");
            eprintln!("E_POLICY_CONFLICT_DENY_WINS_UNENFORCEABLE");
            eprintln!(
                "       {} deny rule(s) conflict with allow rules",
                compat.conflicts.len()
            );
            metrics::increment("policy_conflict_fail_closed");
            return Ok(exit_codes::POLICY_UNENFORCEABLE);
        }

        eprintln!("WARN: Landlock policy conflict detected (Deny rule inside Allow root).");
        eprintln!(
            "WARN: Degrading to Audit mode (no containment). use --fail-closed to make this fatal."
        );
        metrics::increment("degraded_to_audit_conflict");

        should_enforce = false;
    }

    // Spawn child with sandbox isolation
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

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
    }
    // Always set TMPDIR/TMP/TEMP to scoped tmp
    cmd.env("TMPDIR", &tmp_dir);
    cmd.env("TMP", &tmp_dir);
    cmd.env("TEMP", &tmp_dir);

    // Prepare Landlock ruleset in parent process (Safe allocation/IO)
    #[cfg(target_os = "linux")]
    let enforcer_opt = if should_enforce {
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
