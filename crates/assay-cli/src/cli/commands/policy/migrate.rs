use crate::cli::args::PolicyMigrateArgs;
use crate::exit_codes;
use anyhow::{Context, Result};

pub async fn run(args: PolicyMigrateArgs) -> Result<i32> {
    let yaml = if args.coverage {
        assay_core::mcp::policy::McpPolicy::migrate_coverage_yaml(&super::resolved::read_bounded(
            &args.input,
        )?)
    } else {
        let bytes = assay_core::mcp::policy::McpPolicy::read_file(&args.input)?;
        assay_core::mcp::policy::McpPolicy::migrate_yaml(&bytes)
    }
    .with_context(|| format!("failed to migrate policy {}", args.input.display()))?;

    if args.dry_run || args.check {
        if args.coverage {
            print!("{yaml}");
        } else {
            println!("{yaml}");
        }
        return Ok(exit_codes::OK);
    }

    let out_path = args.output.clone().unwrap_or_else(|| args.input.clone());
    std::fs::write(&out_path, yaml)
        .with_context(|| format!("failed to write {}", out_path.display()))?;

    eprintln!("✔ Migrated policy written: {}", out_path.display());
    Ok(exit_codes::OK)
}
