use anyhow::{Context, Result};
use crate::cli::args::PolicyMigrateArgs;
use crate::cli::commands::exit_codes;

pub async fn run(args: PolicyMigrateArgs) -> Result<i32> {
    let mut policy = assay_core::mcp::policy::McpPolicy::from_file(&args.input)
        .with_context(|| format!("failed to load policy {}", args.input.display()))?;

    // Force migration even if from_file already migrated in-memory
    policy.migrate_constraints_to_schemas();

    // Ensure schemas compile after migration
    policy.compile_all_schemas();

    // Serialize to YAML
    let yaml = serde_yaml::to_string(&policy).context("failed to serialize migrated policy")?;

    if args.dry_run {
        println!("{}", yaml);
        return Ok(exit_codes::OK);
    }

    let out_path = args.output.clone().unwrap_or_else(|| args.input.clone());
    std::fs::write(&out_path, yaml).with_context(|| format!("failed to write {}", out_path.display()))?;

    eprintln!(
        "✔ Migrated policy written: {}",
        out_path.display()
    );
    Ok(exit_codes::OK)
}
