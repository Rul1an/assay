use anyhow::{Context, Result};
use crate::cli::args::PolicyValidateArgs;
use crate::cli::commands::exit_codes;

pub async fn run(args: PolicyValidateArgs) -> Result<i32> {
    if args.deny_deprecations {
        std::env::set_var("ASSAY_STRICT_DEPRECATIONS", "1");
    }

    // Let core handle parsing + auto-migration warnings.
    let policy = assay_core::mcp::policy::McpPolicy::from_file(&args.input)
        .with_context(|| format!("failed to load policy {}", args.input.display()))?;

    // Force schema compilation so failures happen here (not at runtime).
    policy
        .compile_all_schemas();

    eprintln!("✔ Policy OK: {}", args.input.display());
    Ok(exit_codes::OK)
}
