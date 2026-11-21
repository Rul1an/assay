use crate::cli::args::MigrateArgs;
use crate::exit_codes;
use anyhow::{Context, Result};
use std::{fs, io::Write, path::Path};

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

    let old_json = serde_json::to_value(&config)?;
    let mut migrated = assay_core::config::resolve::resolve_authored_policies(config, &config_path)
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
    assay_core::config::check_config_size(&config_path, new_yaml.len() as u64)
        .context("migrated config would be too large to read")?;

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

        let directory = backup_path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let mut backup = tempfile::NamedTempFile::new_in(directory)
            .with_context(|| format!("failed to stage backup for {:?}", config_path))?;
        backup
            .write_all(input.original.as_bytes())
            .with_context(|| format!("failed to write backup for {:?}", config_path))?;
        backup.persist_noclobber(&backup_path).with_context(|| {
            format!(
                "failed to create backup without overwriting {:?}",
                backup_path
            )
        })?;
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
    fn migrating_refuses_duplicate_expected_keys_before_writing() -> Result<()> {
        for expected in [
            "{must_contain: [first], must_contain: [last]}",
            "{schema: {Search: {type: string, type: integer}}}",
            "{schema: {Search: {properties: {1: {}, '1': {type: string}}}}}",
        ] {
            let dir = tempfile::tempdir()?;
            let path = dir.path().join("eval.yaml");
            let source = format!(
                "suite: migration\nmodel: trace\ntests:\n  - id: case\n    input: hello\n    expected: {expected}\n"
            );
            fs::write(&path, &source)?;
            let runtime_error = assay_core::config::load_config(&path, false, true)
                .expect_err("the runtime reader must also refuse duplicate keys");
            assert!(
                runtime_error.to_string().contains("duplicate"),
                "{runtime_error}"
            );
            let error = cmd_migrate(MigrateArgs {
                config: path.clone(),
                check: false,
                dry_run: false,
            })
            .expect_err("migration must refuse duplicate keys");
            assert!(format!("{error:#}").contains("duplicate"), "{error:#}");
            assert_eq!(fs::read_to_string(&path)?, source);
            assert!(!path.with_extension("yaml.bak").exists());
        }
        Ok(())
    }

    #[test]
    fn migration_keeps_distinct_property_names_that_yaml_types_alike() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let path = dir.path().join("eval.yaml");
        let source = "suite: migration\nmodel: trace\ntests:\n  - id: case\n    input: hello\n    expected: {schema: {Search: {properties: {true: {}, True: {type: string}}}}}\n";
        fs::write(&path, source)?;
        assert_eq!(
            cmd_migrate(MigrateArgs {
                config: path.clone(),
                check: false,
                dry_run: false,
            })?,
            exit_codes::OK
        );
        let config = assay_core::config::load_config(&path, false, true)?;
        let expected = serde_json::to_value(&config.tests[0].expected)?;
        let properties = &expected["schema"]["schemas"]["Search"]["properties"];
        assert_eq!(properties["true"], serde_json::json!({}));
        assert_eq!(properties["True"], serde_json::json!({"type": "string"}));
        Ok(())
    }

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
    fn duplicate_inline_expectation_keys_are_refused_before_migration() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("eval.yaml");
        for expected in [
            "{must_contain: [first], must_contain: [second]}",
            "{schema: {Search: {type: string, type: integer}}}",
            "{schema: {Search: {properties: {1: {}, '1': {type: string}}}}}",
            "[{type: must_contain, must_contain: [first], must_contain: [second]}]",
            "{type: args_valid, schema: {version: '2', tools: {deny: [rm], deny: [other]}}}",
        ] {
            let original = format!("suite: migration\nmodel: trace\ntests:\n- id: t\n  input: p\n  expected: {expected}\n");
            fs::write(&config, &original)?;
            let error = cmd_migrate(MigrateArgs {
                config: config.clone(),
                check: false,
                dry_run: false,
            })
            .expect_err("migration dropped an inline expectation key");
            assert!(
                format!("{error:#}").contains("duplicate member"),
                "{error:#}"
            );
            assert_eq!(fs::read_to_string(&config)?, original);
            assert!(!directory.path().join("eval.yaml.bak").exists());
        }
        Ok(())
    }

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
  expected: {type: json_schema, json_schema: "", schema_file: absent/../answer.json}
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
        assert!(
            migrated.contains("schema_file: absent/../answer.json"),
            "{migrated}"
        );
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

#[cfg(test)]
mod referenced_expected_tests {
    use super::*;

    #[test]
    fn an_existing_backup_can_be_a_referenced_input() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("eval.yaml");
        let backup = directory.path().join("eval.yaml.bak");
        let original = "suite: migration\nmodel: trace\ntests:\n- id: t\n  input: p\n  expected: {$ref: eval.yaml.bak}\n";
        let assertion = "must_contain: [sentinel]\n";
        fs::write(&config, original)?;
        fs::write(&backup, assertion)?;
        let migrate = |check, dry_run| {
            cmd_migrate(MigrateArgs {
                config: config.clone(),
                check,
                dry_run,
            })
        };
        assert_eq!(migrate(true, false)?, exit_codes::CONFIG_ERROR);
        assert_eq!(migrate(false, true)?, exit_codes::OK);
        let error = migrate(false, false).expect_err("migration overwrote its referenced input");
        assert!(format!("{error:#}").contains("eval.yaml.bak"), "{error:#}");
        assert_eq!(fs::read_to_string(config)?, original);
        assert_eq!(fs::read_to_string(backup)?, assertion);
        Ok(())
    }

    #[test]
    fn duplicate_and_oversized_references_leave_inputs_intact() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let config = directory.path().join("eval.yaml");
        let reference = directory.path().join("part.yaml");
        let backup = directory.path().join("eval.yaml.bak");
        let original = "suite: migration\nmodel: trace\ntests:\n- id: t\n  input: p\n  expected: {$ref: part.yaml}\n";
        let migrate = || {
            cmd_migrate(MigrateArgs {
                config: config.clone(),
                check: false,
                dry_run: false,
            })
        };
        let mut at_limit = String::from("must_contain: [first]\n#");
        let limit = assay_core::model::MAX_POLICY_INPUT_BYTES as usize;
        at_limit.push_str(&"x".repeat(limit - at_limit.len()));
        for (source, reason) in [
            (
                "must_contain: [first]\nmust_contain: [second]".to_owned(),
                "duplicate member",
            ),
            (format!("{at_limit}x"), "source bytes limit"),
        ] {
            fs::write(&config, original)?;
            fs::write(&reference, &source)?;
            let error = migrate().expect_err("invalid reference was migrated");
            let message = format!("{error:#}");
            assert!(
                message.contains(reason) && message.contains("part.yaml"),
                "{message}"
            );
            assert_eq!(fs::read_to_string(&config)?, original);
            assert_eq!(fs::read_to_string(&reference)?, source);
            assert!(!backup.exists());
        }
        fs::write(&reference, &at_limit)?;
        assert_eq!(migrate()?, exit_codes::OK);
        assert!(fs::read_to_string(&config)?.contains("first"));
        assert_eq!(fs::read_to_string(backup)?, original);
        assert_eq!(fs::read_to_string(reference)?, at_limit);
        Ok(())
    }
}
