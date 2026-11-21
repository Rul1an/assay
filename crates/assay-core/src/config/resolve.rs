use super::path_resolver::PathResolver;
use crate::model::{EvalConfig, Expected};
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub fn resolve_policies(config: EvalConfig, base_dir: &Path) -> Result<EvalConfig> {
    resolve_policies_inner(config, base_dir, None)
}

/// Resolve policies in an authored config before its runtime paths have been normalized.
#[doc(hidden)]
pub fn resolve_authored_policies(config: EvalConfig, config_path: &Path) -> Result<EvalConfig> {
    let base_dir = config_path.parent().unwrap_or(Path::new("."));
    resolve_policies_inner(config, base_dir, Some(PathResolver::new(config_path)))
}

fn resolve_policies_inner(
    mut config: EvalConfig,
    base_dir: &Path,
    authored_paths: Option<PathResolver>,
) -> Result<EvalConfig> {
    for test in &mut config.tests {
        // Resolve the referenced block before migrating its policy. Its paths must be
        // written relative to the config even when the operator supplied a directory prefix.
        let referenced_block = matches!(test.expected, Expected::Reference { .. });
        if let (Some(paths), Expected::Reference { path }) = (&authored_paths, &test.expected) {
            test.expected = read_expected_reference(base_dir, path, true)?;
            if let Expected::JsonSchema {
                schema_file: Some(file),
                ..
            } = &test.expected
            {
                // Opening a reference follows symlinks. A lexical link/.. reduction by the
                // runtime must not redirect the migrated assertion to a different schema.
                let source = base_dir.join(file);
                let mut runtime = file.clone();
                paths.resolve_str(&mut runtime);
                // Path equality ignores trailing slashes even when opening a file does not.
                if source.as_os_str() != Path::new(&runtime).as_os_str() {
                    // Some platforms canonicalize a trailing slash away even when stat
                    // rejects the authored spelling with ENOTDIR.
                    std::fs::metadata(&source)
                        .with_context(|| format!("cannot access nested schema_file {file}"))?;
                    let identity = |p: &Path| {
                        p.canonicalize().with_context(|| {
                            format!("cannot resolve nested schema_file {}", p.display())
                        })
                    };
                    anyhow::ensure!(identity(&source)? == identity(Path::new(&runtime))?,
                        "nested schema_file {file} resolves to a different file after path normalization");
                }
            }
        }
        let policy_dir = |path: &str, file_only: bool| {
            // Older converters also accepted file-only policies beside the config.
            // A dangling link in the working directory is still the selected entry.
            if authored_paths.is_none()
                || referenced_block
                || (file_only
                    && matches!(std::fs::symlink_metadata(path), Err(e) if e.kind() == std::io::ErrorKind::NotFound))
            {
                base_dir
            } else {
                Path::new(".")
            }
        };
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "only variants carrying a policy reference need resolving; a new one that carries a path must be added above or its path is never resolved"
        )]
        match &mut test.expected {
            Expected::JsonSchema {
                json_schema,
                schema_file,
            } if schema_file.is_some() => {
                if let Some(paths) = &authored_paths {
                    // Validation opens the file. Serialization must keep its authored relative path.
                    let mut validation_path = schema_file.clone();
                    paths.resolve_opt_str(&mut validation_path);
                    let expected = Expected::JsonSchema {
                        json_schema: json_schema.clone(),
                        schema_file: validation_path,
                    };
                    crate::model::validate_expected_for_execution(&expected).with_context(
                        || format!("test '{}': resolved expected block is invalid", test.id),
                    )?;
                    continue;
                }
            }
            Expected::ArgsValid {
                policy,
                schema: Some(_),
            } if authored_paths.is_some() => {
                // The inline schema determines the verdict; removing the old path changes the cache fingerprint.
                *policy = None;
            }
            Expected::ArgsValid {
                ref mut policy,
                ref mut schema,
            } if schema.is_none() => {
                if let Some(path) = policy {
                    let read = if authored_paths.is_some() {
                        crate::model::read_args_file_for_migration
                    } else {
                        crate::model::read_args_policy_file
                    };
                    *schema =
                        Some(read(&policy_dir(path, true).join(&*path)).with_context(|| {
                            format!("failed to load args_valid policy: {path}")
                        })?);
                    *policy = None;
                }
            }
            Expected::SequenceValid {
                ref mut policy,
                ref mut sequence,
                ref mut rules,
            } if authored_paths.is_some() || (sequence.is_none() && rules.is_none()) => {
                if let Some(path) = policy {
                    let input =
                        policy_dir(path, sequence.is_none() && rules.is_none()).join(&*path);
                    let policy_content =
                        crate::model::read_policy_text(&input).with_context(|| {
                            format!("failed to read policy file: {}", input.display())
                        })?;

                    // The same parse the binding uses before a test runs. Inline rules cannot
                    // carry aliases or any other policy field either, so a file that declares
                    // them is refused here too.
                    match crate::model::parse_sequence_policy_file(&policy_content) {
                        Ok(loaded) => loaded.validated()?.fill_missing(sequence, rules),
                        Err(refused) => match refused.into_refusal(path) {
                            Ok(refusal) => return Err(refusal),
                            Err(parse_error) => anyhow::bail!("Failed to parse sequence policy '{}' as a list of strings, a structured policy, or a list of rules: {}", path, parse_error),
                        },
                    }

                    *policy = None;
                }
            }
            Expected::Reference { path } if authored_paths.is_none() => {
                test.expected = read_expected_reference(base_dir, path, false)?;
            }
            _ => {}
        }

        crate::model::validate_test_case_for_execution(test)
            .with_context(|| format!("test '{}': resolved expected block is invalid", test.id))?;
        if let (
            Some(_),
            Expected::ArgsValid {
                schema: Some(schema),
                ..
            },
        ) = (&authored_paths, &mut test.expected)
        {
            use crate::model::ArgsPolicyView;
            match crate::model::validated_args_policy(schema)? {
                ArgsPolicyView::SchemaMap(_) => {
                    // A plain map ignores unlisted tools, as a structured policy does by default.
                    *schema = serde_json::json!({"version": "2.0", "schemas": schema.take()});
                    crate::model::validate_args_policy_value(schema).with_context(|| {
                        format!("test '{}': migrated argument policy is invalid", test.id)
                    })?;
                }
                ArgsPolicyView::Structured { allow, deny, .. } => {
                    let root = schema
                        .as_object_mut()
                        .context("structured argument policy must be a mapping")?;
                    // Use the evaluator's combined lists, including their order and duplicates.
                    for (key, list) in [("allow", allow), ("deny", deny)] {
                        if root.remove(key).is_some() {
                            root.entry("tools").or_insert_with(|| serde_json::json!({}))[key] =
                                serde_json::json!(list);
                        }
                    }
                }
            }
        }
    }

    // Auto-bump version if resolving?
    // The user plan says: "Dit doet dezelfde path→inline transformatie".
    // It doesn't explicitly say it bumps version, but keeping it consistent with migration is good.
    // However, for "mixed mode" support, we might just resolved policies without enforcing version=1?
    // User request: "Precedence rule: if configVersion: 1 and schema/rules/blocklist present → use inline... If configVersion: 1 but only policy present → allowed"
    // So current load logic handles precedence (by checking fields).
    // `resolve_policies` transforms config to have inline fields.

    // Let's NOT bump version automatically here, let the caller decide (migration command bumps it).
    // But for equivalence tests we want them equal.

    Ok(config)
}

fn read_expected_reference(base_dir: &Path, path: &str, authored: bool) -> Result<Expected> {
    let mut resolved = crate::model::read_referenced_expected(&base_dir.join(path))?;
    let reference_path = if authored {
        PathBuf::from(path)
    } else {
        base_dir.join(path)
    };
    resolve_nested_expected_paths(&mut resolved, &reference_path, authored);
    Ok(resolved)
}

fn resolve_nested_expected_paths(expected: &mut Expected, reference_path: &Path, authored: bool) {
    let base = reference_path.parent().unwrap_or(Path::new("."));
    let resolve = |path: &mut String| {
        let candidate = Path::new(path);
        if !candidate.is_absolute() {
            let joined = base.join(candidate);
            *path = if authored && joined.is_relative() && std::path::MAIN_SEPARATOR != '/' {
                joined
                    .to_string_lossy()
                    .replace(std::path::MAIN_SEPARATOR, "/")
            } else {
                joined.to_string_lossy().into_owned()
            };
        }
    };
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "nested resolution applies to the variants that hold a nested path; the rest have nothing to walk"
    )]
    match expected {
        Expected::JsonSchema {
            schema_file: Some(path),
            ..
        }
        | Expected::ArgsValid {
            policy: Some(path), ..
        }
        | Expected::SequenceValid {
            policy: Some(path), ..
        } => resolve(path),
        _ => {}
    }
}
