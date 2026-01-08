use anyhow::{Context, Result};
use crate::cli::args::PolicyFmtArgs;
use crate::cli::commands::exit_codes;

pub async fn run(args: PolicyFmtArgs) -> Result<i32> {
    // Parse (auto-migration optional here; fmt implies just formatting but keeping semantics)
    let policy = assay_core::mcp::policy::McpPolicy::from_file(&args.input)
        .with_context(|| format!("failed to load policy {}", args.input.display()))?;

    // Normalize output
    let yaml = serde_yaml::to_string(&policy).context("failed to serialize policy")?;

    let out_path = args.output.clone().unwrap_or_else(|| args.input.clone());
    std::fs::write(&out_path, yaml).with_context(|| format!("failed to write {}", out_path.display()))?;

    eprintln!("✔ Formatted: {}", out_path.display());
    Ok(exit_codes::OK)
}
