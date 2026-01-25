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

    /// Dry-run mode: Log violations but do not block
    #[arg(long)]
    pub dry_run: bool,

    /// Trace level: error|warn|info|debug|trace
    #[arg(long, default_value = "info")]
    pub trace_level: String,
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

    eprintln!("------------------");

    // Spawn child with sandbox isolation
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

    let status = spawn_sandboxed(cmd_name, cmd_args, &backend, &policy)?;

    match status.code() {
        Some(0) => Ok(exit_codes::SUCCESS),
        Some(_) => Ok(exit_codes::COMMAND_FAILED),
        None => {
            eprintln!("sandbox error: child terminated by signal");
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
) -> anyhow::Result<std::process::ExitStatus> {
    let mut cmd = std::process::Command::new(cmd_name);
    cmd.args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    // Apply Landlock isolation via pre_exec (child-side, before exec)
    #[cfg(all(target_os = "linux", target_family = "unix"))]
    {
        use std::os::unix::process::CommandExt;

        if matches!(backend, BackendType::Landlock) {
            let policy_clone = policy.clone();
            // SAFETY: pre_exec runs after fork, before exec in child process.
            // apply_landlock only calls Landlock syscalls (no async, no allocations).
            unsafe {
                cmd.pre_exec(move || {
                    crate::backend::apply_landlock(&policy_clone)
                        .map_err(|e| std::io::Error::other(e.to_string()))?;
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
    }

    let status = cmd
        .status()
        .map_err(|e| anyhow::anyhow!("failed to spawn child: {}", e))?;

    Ok(status)
}
