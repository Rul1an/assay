use crate::backend::BackendType;
use crate::cli::args::SandboxArgs;
use crate::env_filter::{format_banner, EnvFilter, EnvMode};
use crate::exit_codes;
use std::process::Stdio;

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

    // PR5.1: Environment filtering
    let (env_filter, env_mode) = build_env_filter(&args);
    let env_result = env_filter.filter_current();
    eprintln!("  Env: {}", format_banner(&env_result, env_mode));

    // PR2: Load policy from file or use default MCP pack
    let policy = if let Some(ref path) = args.policy {
        match crate::policy::Policy::load(path) {
            Ok(p) => {
                eprintln!("Policy:  {} (loaded)", path.display());
                p
            }
            Err(e) => {
                eprintln!("WARN: Failed to load policy: {}. Using default.", e);
                crate::policy::mcp_server_minimal()
            }
        }
    } else {
        eprintln!("Policy:  mcp-server-minimal (default)");
        crate::policy::mcp_server_minimal()
    };

    let (fs_allow, fs_deny, net_allow, net_deny) = policy.rule_counts();
    eprintln!(
        "Rules:   FS(allow:{} deny:{}) NET(allow:{} deny:{})",
        fs_allow, fs_deny, net_allow, net_deny
    );
    eprintln!("Command: {:?}", args.command);
    eprintln!("PID:     {}", std::process::id());

    // PR1: Ensure trace directory exists
    match crate::fs::ensure_assay_trace_dir() {
        Ok(path) => eprintln!("Traces:  {}", path.display()),
        Err(e) => eprintln!("WARN: Failed to create trace dir: {}", e),
    }

    // PR5.4: Scoped /tmp with proper isolation
    // Use UID (not $USER env which can be spoofed) + PID for uniqueness per run
    let tmp_dir = create_scoped_tmp()?;
    eprintln!("Tmp:     {}", tmp_dir.display());

    eprintln!("──────────────────");

    // Spawn child with sandbox isolation
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

    let status = spawn_sandboxed(
        cmd_name,
        cmd_args,
        &backend,
        &policy,
        &tmp_dir,
        &env_result.filtered_env,
    )?;

    // Consolidate exit code logic
    // Just return direct status code or signal failure
    match status.code() {
        Some(code) => Ok(code),
        None => {
            eprintln!("sandbox error: child terminated by signal");
            // Standard convention 128 + signal, but we can return generic error for now
            Ok(exit_codes::INTERNAL_ERROR)
        }
    }
}

/// Build the environment filter based on CLI args.
fn build_env_filter(args: &SandboxArgs) -> (EnvFilter, EnvMode) {
    if args.env_passthrough {
        (EnvFilter::passthrough(), EnvMode::Passthrough)
    } else {
        let filter = EnvFilter::default_scrub();
        let filter = if let Some(ref allowed) = args.env_allow {
            filter.with_allowed(allowed)
        } else {
            filter
        };
        (filter, EnvMode::Scrub)
    }
}

/// Spawn a child process with appropriate sandbox isolation.
/// On Linux with Landlock: applies restrictions via pre_exec (before exec).
/// On other platforms: just runs the command (audit-only).
fn spawn_sandboxed(
    cmd_name: &str,
    cmd_args: &[String],
    backend: &BackendType,
    policy: &crate::policy::Policy,
    scoped_tmp: &std::path::Path,
    filtered_env: &std::collections::HashMap<String, String>,
) -> anyhow::Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new(cmd_name);
    cmd.args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // PR5.1: Apply filtered environment (scrubbed by default)
    cmd.env_clear();
    for (key, value) in filtered_env {
        cmd.env(key, value);
    }
    // Always set TMPDIR to scoped tmp
    cmd.env("TMPDIR", scoped_tmp);

    // Check Landlock compatibility (PR5.2)
    // Detect "Deny inside Allow" conflicts which Landlock cannot enforce.
    let cwd = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    let compat = crate::landlock_check::check_compatibility(policy, &cwd, scoped_tmp);

    #[allow(unused_assignments, unused_mut)]
    let mut should_enforce = matches!(backend, BackendType::Landlock);

    if should_enforce && !compat.is_compatible() {
        eprintln!("WARN: Landlock policy conflict detected (Deny rule inside Allow root).");
        for (allow, deny) in &compat.conflicts {
            eprintln!("  - Deny {:?} is inside Allowed {:?}", deny, allow);
        }
        eprintln!("WARN: Degrading to Audit mode (no containment). Fix policy or use Landlock-compatible rules.");
        #[cfg(target_os = "linux")]
        {
            should_enforce = false;
        }
    }

    // Prepare Landlock ruleset in parent process (Safe allocation/IO)
    #[cfg(target_os = "linux")]
    let enforcer_opt = if should_enforce {
        Some(crate::backend::prepare_landlock(policy, scoped_tmp)?)
    } else {
        None
    };

    // Apply Landlock isolation via pre_exec (child-side, before exec)
    #[cfg(all(target_os = "linux", target_family = "unix"))]
    {
        use std::os::unix::process::CommandExt;

        if let Some(mut enforcer) = enforcer_opt {
            // SAFETY: pre_exec runs after fork, before exec in child process.
            // LandlockHelper::enforce() only calls restrict_self() (syscalls only).
            // No allocations or IO occur in the critical section.
            unsafe {
                cmd.pre_exec(move || {
                    enforcer.enforce()?;
                    Ok(())
                });
            }
        }
    }

    // Suppress unused variable warning on non-Linux
    #[cfg(not(target_os = "linux"))]
    {
        let _ = backend;
        let _ = policy;
        let _ = scoped_tmp;
    }

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn child: {}", e))?;

    Ok(status)
}

/// Create a scoped temporary directory for sandbox isolation.
///
/// # Security
/// - Uses UID (not $USER env var which can be spoofed)
/// - Adds PID for uniqueness per sandbox run
/// - Sets 0700 permissions (owner-only access)
/// - Prefers XDG_RUNTIME_DIR if available (often tmpfs, more secure)
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
