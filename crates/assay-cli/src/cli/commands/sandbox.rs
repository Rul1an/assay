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
    eprintln!("Backend: [No-Op Stub]"); // Placeholder for PR3
    eprintln!(
        "Policy:  {:?}",
        args.policy
            .as_deref()
            .unwrap_or(std::path::Path::new("default"))
    );
    eprintln!("Command: {:?}", args.command);
    eprintln!("PID:     {}", std::process::id());

    // PR1: Ensure trace directory exists
    match crate::fs::ensure_assay_trace_dir() {
        Ok(path) => eprintln!("Traces:  {}", path.display()),
        Err(e) => eprintln!("WARN: Failed to create trace dir: {}", e),
    }

    eprintln!("------------------");

    // v0.1 Skeleton: Just spawn the child directly (no isolation yet)
    let cmd_name = &args.command[0];
    let cmd_args = &args.command[1..];

    let status = std::process::Command::new(cmd_name)
        .args(cmd_args)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status();

    match status {
        Ok(s) => {
            if s.success() {
                Ok(exit_codes::SUCCESS)
            } else {
                Ok(exit_codes::COMMAND_FAILED)
            }
        }
        Err(e) => {
            eprintln!("sandbox error: failed to spawn child: {}", e);
            Ok(exit_codes::INTERNAL_ERROR)
        }
    }
}
