use crate::cli::args::PolicyFmtArgs;
use crate::exit_codes;
use anyhow::{Context, Result};

pub async fn run(args: PolicyFmtArgs) -> Result<i32> {
    let bytes = assay_core::mcp::policy::McpPolicy::read_file(&args.input)?;
    let yaml = assay_core::mcp::policy::McpPolicy::format_yaml(&bytes)
        .with_context(|| format!("failed to format policy {}", args.input.display()))?;

    let out_path = args.output.clone().unwrap_or_else(|| args.input.clone());
    std::fs::write(&out_path, yaml)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    eprintln!("✔ Formatted: {}", out_path.display());
    Ok(exit_codes::OK)
}
