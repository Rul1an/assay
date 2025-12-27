use super::exit_codes;
use crate::cli::args::MigrateArgs;
use anyhow::{Context, Result};
use assay_core::model::EvalConfig;
use std::fs;

pub fn cmd_migrate(args: MigrateArgs) -> Result<i32> {
    let config_path = args.config;
    println!("Migrating configuration: {:?}", config_path);

    if !config_path.exists() {
        anyhow::bail!("Config file not found: {:?}", config_path);
    }

    // Read original content for backup later
    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read config: {:?}", config_path))?;

    // Parse config
    let mut config: EvalConfig =
        serde_yaml::from_str(&content).context("failed to parse config YAML")?;
    println!(
        "Loaded config version: {} (model: {}, suite: {})",
        config.version, config.model, config.suite
    );

    // Use core resolver to inline policies
    // We treat the config file's parent as base dir
    let base_dir = config_path.parent().unwrap_or(std::path::Path::new("."));

    // Check if any legacy policies exist before resolving?
    // resolve_policies iterates anyway.
    let resolved = assay_core::config::resolve::resolve_policies(config.clone(), base_dir)
        .context("failed to resolve policies")?;

    // Detect if modification happened by comparing
    // Or check if config.version == 0

    // We can check if `resolved` differs from `config` fields.
    // Hack: serialize both to value and compare? Or just implement eq?
    // Or we can rely on `resolve_policies` to return modified flag?
    // The current signature returns `Result<EvalConfig>`.
    // I can assume if version update is needed, I update it.

    // Actually, `resolve_policies` modifies inplace if I pass ownership.
    // If I compare resolved vs original, I know.

    // Assuming resolve_policies handles all inlining.
    // If version is 0, we upgrade to 1.

    let mut new_config = resolved;
    let mut modified = false;

    // Check if we effectively changed anything related to policy inlining.
    // If new_config != config (ignoring version change which we haven't done yet).
    // Eq implementation might be heavy.
    // But verify logic:
    // If `resolve_policies` found external files, it inlined them.
    // The previous implementation printed "Inlining ...".
    // I should probably move the printing to `resolve_policies` or just accept silent operation?
    // CLI users like feedback.
    // Maybe `resolve_policies` should take a callback or return log?
    // Or I just print "Policies resolved." if subsequent check shows fields changed.

    // To restore detailed logging, I would need `resolve_policies` to support it.
    // For now, let's just proceed.

    if new_config.version == 0 {
        // We mandate upgrade if we are running migrate command
        new_config.version = 1;
        modified = true;
    }

    // Also check if any policy loading happened.
    // We can check if any `Expected` changed.
    // This is getting complicated to detect "modified" accurately without the loop.
    // But `migrate` command implies intention.

    // If config was already version 1 and fully inlined, `resolve_policies` returns identical config.
    // `modified` would be true only if version changed?
    // If version was 1, we don't set modified=true based on version.

    // Let's rely on JSON comparison for "effective change".
    let old_json = serde_json::to_value(&config)?;
    let new_json = serde_json::to_value(&new_config)?;

    if old_json != new_json {
        modified = true;
    }

    if !modified {
        println!("No legacy policies found. Config is already up to date.");
        return Ok(exit_codes::OK);
    }

    // Bump config version is handled above
    config = new_config; // Update local var to migrated one
                         // config.version = 1; // Already done if needed

    let new_yaml = serde_yaml::to_string(&config)?;

    if args.dry_run {
        println!("\n--- Migrated Config (Dry Run) ---");
        println!("{}", new_yaml);
    } else {
        // Backup
        let backup_path = {
            let mut p = config_path.clone();
            if let Some(ext) = p.extension() {
                let mut s = ext.to_os_string();
                s.push(".bak");
                p.set_extension(s);
            } else {
                p.set_extension("bak");
            }
            p
        };

        fs::write(&backup_path, &content)
            .with_context(|| format!("failed to write backup: {:?}", backup_path))?;
        println!("Original config backed up to: {:?}", backup_path);

        fs::write(&config_path, new_yaml)
            .with_context(|| format!("failed to write migrated config: {:?}", config_path))?;
        println!("✅ Migration complete! Updated {:?}", config_path);
    }

    Ok(exit_codes::OK)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_migrate_logic() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config_path = dir.path().join("mcp-eval.yaml");
        let policy_path = dir.path().join("args.yaml");

        std::fs::write(
            &policy_path,
            r#"
type: object
properties:
  foo:
    type: string
"#,
        )?;

        std::fs::write(
            &config_path,
            r#"
suite: legacy-migration
model: dummy
tests:
  - id: t1
    input:
       prompt: "hi"
    expected:
       type: args_valid
       policy: args.yaml
"#,
        )?;

        let args = MigrateArgs {
            config: config_path.clone(),
            dry_run: false,
        };

        // We run the cmd logic directly
        let code = cmd_migrate(args)?;
        assert_eq!(code, 0);

        let content = std::fs::read_to_string(&config_path)?;
        println!("Migrated content:\n{}", content);

        assert!(content.contains("configVersion: 1"));
        assert!(!content.contains("policy: args.yaml"));
        assert!(content.contains("schema:"));
        assert!(content.contains("type: object"));

        // Check backup
        let mut bak = config_path.clone();
        bak.set_extension("yaml.bak"); // Logic in code: ext + ".bak" -> .yaml.bak
                                       // Wait, logic is: let mut s = ext.to_os_string(); s.push(".bak"); p.set_extension(s);
                                       // "yaml" -> "yaml.bak". So "mcp-eval.yaml.bak". Correct.
        if !bak.exists() {
            // Fallback check if extension replacement happened differently
            let bak2 = config_path.with_extension("bak");
            if bak2.exists() {
                bak = bak2;
            }
        }
        assert!(bak.exists(), "Backup file should exist at {:?}", bak);

        Ok(())
    }
}
