use crate::cli::args::MigrateArgs;
use crate::exit_codes;
use anyhow::{Context, Result};
use std::fs;

pub fn cmd_migrate(args: MigrateArgs) -> Result<i32> {
    let config_path = args.config;
    println!("Migrating configuration: {:?}", config_path);

    if !config_path.exists() {
        anyhow::bail!("Config file not found: {:?}", config_path);
    }

    let input = assay_core::config::load_config_for_migration(&config_path)
        .context("failed to load config (strict check failed)")?;
    let config = input.config;

    println!(
        "Loaded config version: {} (model: {}, suite: {})",
        config.version, config.model, config.suite
    );

    let base_dir = config_path.parent().unwrap_or(std::path::Path::new("."));
    let old_json = serde_json::to_value(&config)?;
    let mut migrated = assay_core::config::resolve::resolve_authored_policies(config, base_dir)
        .context("failed to resolve policies")?;
    if migrated.version == 0 {
        migrated.version = assay_core::config::SUPPORTED_CONFIG_VERSION;
    }
    let modified = input.legacy_expected || old_json != serde_json::to_value(&migrated)?;

    if args.check {
        if modified {
            eprintln!(
                "Migration required for {:?}. Run 'assay migrate' to update configuration.",
                config_path
            );
            return Ok(exit_codes::CONFIG_ERROR);
        } else {
            println!("Config {:?} is clean (already migrated).", config_path);
            return Ok(exit_codes::OK);
        }
    }

    if !modified {
        println!("No legacy policies found. Config is already up to date.");
        return Ok(exit_codes::OK);
    }

    let new_yaml = serde_yaml::to_string(&migrated)?;

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

        fs::write(&backup_path, &input.original)
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

    #[test]
    fn test_migrate_logic() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let config_path = dir.path().join("mcp-eval.yaml");
        let policy_path = dir.path().join("args.yaml");

        std::fs::write(
            &policy_path,
            r#"
Search:
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

        // unused imports removed
        let args = MigrateArgs {
            check: false,
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
        assert!(content.contains("Search:"));
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

#[cfg(test)]
mod legacy_expected_tests {
    use super::*;

    #[test]
    fn migrating_a_sibling_assertion_keeps_schema_files_relative_to_the_config() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("eval.yaml");
        fs::write(directory.path().join("answer.json"), r#"{"type":"string"}"#)?;
        fs::write(
            &config,
            r#"configVersion: 1
suite: migration
model: trace
tests:
- id: legacy
  input: capital
  expected: {must_contain: [Paris]}
- id: schema
  input: answer
  expected: {type: json_schema, json_schema: "", schema_file: answer.json}
"#,
        )?;
        assert_eq!(
            cmd_migrate(MigrateArgs {
                config: config.clone(),
                check: false,
                dry_run: false
            })?,
            exit_codes::OK
        );
        let migrated = fs::read_to_string(&config)?;
        assert!(migrated.contains("schema_file: answer.json"), "{migrated}");
        assert!(!migrated.contains("schema_file_resolved"), "{migrated}");
        Ok(())
    }

    #[test]
    fn versioned_config_still_migrates_untagged_expectations() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("eval.yaml");
        let backup = directory.path().join("eval.yaml.bak");
        let migrate = |check, dry_run| {
            cmd_migrate(MigrateArgs {
                config: config.clone(),
                check,
                dry_run,
            })
        };

        for expected in ["{must_contain: [Paris]}", "[{must_contain: [Paris]}]"] {
            let original = format!(
                "configVersion: 1\nsuite: migration\nmodel: trace\ntests:\n  - id: capital\n    input: {{prompt: capital}}\n    expected: {expected}\n"
            );
            fs::write(&config, &original)?;
            assert_eq!(
                migrate(true, false)?,
                exit_codes::CONFIG_ERROR,
                "{expected}"
            );
            assert_eq!(migrate(false, true)?, exit_codes::OK);
            assert_eq!(fs::read_to_string(&config)?, original);
            assert!(!backup.exists());

            assert_eq!(migrate(false, false)?, exit_codes::OK);
            let migrated = fs::read_to_string(&config)?;
            assert!(migrated.contains("type: must_contain"));
            assert!(migrated.contains("Paris"));
            assert_eq!(fs::read_to_string(&backup)?, original);
            assert_eq!(migrate(true, false)?, exit_codes::OK);
            assert_eq!(migrate(false, false)?, exit_codes::OK);
            assert_eq!(fs::read_to_string(&config)?, migrated);
            assert_eq!(fs::read_to_string(&backup)?, original);
            fs::remove_file(&backup)?;
        }
        Ok(())
    }
}
