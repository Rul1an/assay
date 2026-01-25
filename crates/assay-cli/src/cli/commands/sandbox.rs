use crate::backend::BackendType;
use crate::exit_codes;
use clap::Args;
use std::path::PathBuf;
use std::process::Stdio;

#[derive(Args, Debug, Clone)]
pub struct SandboxArgs {
    /// Command to run in the sandbox
    #[arg(allow_hyphen_values = true, required = true, trailing_var_arg = true)]
    pub command: Vec<String>,

    /// Path to policy file (optional)
    #[arg(long)]
    pub policy: Option<PathBuf>,
}

pub async fn run(args: SandboxArgs) -> anyhow::Result<i32> {
    eprintln!("Assay Sandbox v0.1");
    eprintln!("------------------");

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

    // Scoped /tmp creation
    let user = std::env::var("USER").unwrap_or_else(|_| "sandbox".to_string());
    let tmp_dir = std::path::PathBuf::from(format!("/tmp/assay-{}", user));
    if let Err(e) = std::fs::create_dir_all(&tmp_dir) {
        eprintln!(
            "WARN: Failed to create scoped tmp dir {}: {}",
            tmp_dir.display(),
            e
        );
    }

    eprintln!("------------------");

    // Spawn child with sandbox isolation
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

    let status = spawn_sandboxed(cmd_name, cmd_args, &backend, &policy, &tmp_dir)?;

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

/// Spawn a child process with appropriate sandbox isolation.
/// On Linux with Landlock: applies restrictions via pre_exec (before exec).
/// On other platforms: just runs the command (audit-only).
fn spawn_sandboxed(
    cmd_name: &str,
    cmd_args: &[String],
    backend: &BackendType,
    policy: &crate::policy::Policy,
    scoped_tmp: &std::path::Path,
) -> anyhow::Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new(cmd_name);
    cmd.args(cmd_args)
        .env("TMPDIR", scoped_tmp)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Prepare Landlock ruleset in parent process (Safe allocation/IO)
    #[cfg(target_os = "linux")]
    let mut enforcer = crate::backend::prepare_landlock(policy, scoped_tmp)?;

    // Apply Landlock isolation via pre_exec (child-side, before exec)
    #[cfg(all(target_os = "linux", target_family = "unix"))]
    {
        use std::os::unix::process::CommandExt;

        if matches!(backend, BackendType::Landlock) {
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
