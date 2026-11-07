use crate::model::{EvalConfig, Expected};
use anyhow::{Context, Result};
use std::path::Path;

pub fn resolve_policies(mut config: EvalConfig, base_dir: &Path) -> Result<EvalConfig> {
    for test in &mut config.tests {
        #[expect(
            clippy::wildcard_enum_match_arm,
            reason = "only variants carrying a policy reference need resolving; a new one that carries a path must be added above or its path is never resolved"
        )]
        match &mut test.expected {
            Expected::ArgsValid {
                ref mut policy,
                ref mut schema,
            } if schema.is_none() => {
                if let Some(path) = policy {
                    let policy_content = read_policy_file(base_dir, path)?;
                    let loaded: serde_json::Value = serde_yaml::from_str(&policy_content)
                        .with_context(|| format!("failed to parse policy YAML: {}", path))?;

                    if crate::model::has_structured_args_policy_shape(&loaded) {
                        anyhow::bail!(
                            "migration does not serialize structured args_valid policy '{}' into the public inline schema form; keep the policy reference",
                            path
                        );
                    }

                    *schema = Some(loaded);
                    *policy = None;
                }
            }
            Expected::SequenceValid {
                ref mut policy,
                ref mut sequence,
                ref mut rules,
            } if sequence.is_none() && rules.is_none() => {
                if let Some(path) = policy {
                    let policy_content = read_policy_file(base_dir, path)?;

                    // The same parse the binding uses before a test runs. Inline rules cannot
                    // carry aliases either, so a file that declares them is refused here too.
                    match crate::model::parse_sequence_policy_file(&policy_content) {
                        Ok(crate::model::SequencePolicyFile::ExactSequence(loaded)) => {
                            *sequence = Some(loaded);
                        }
                        Ok(crate::model::SequencePolicyFile::Rules(loaded)) => {
                            *rules = Some(loaded);
                        }
                        Err(crate::model::SequencePolicyFileError::DeclaresAliases(names)) => {
                            return Err(crate::model::SequencePolicyFileError::alias_refusal(
                                path, &names,
                            ));
                        }
                        Err(crate::model::SequencePolicyFileError::Invalid(_)) => {
                            anyhow::bail!("Failed to parse sequence policy '{}' as a list of strings, a structured policy, or a list of rules", path);
                        }
                    }

                    *policy = None;
                }
            }
            Expected::Reference { path } => {
                let policy_content = read_policy_file(base_dir, path)?;
                let reference_path = base_dir.join(&*path);
                let value: serde_json::Value = serde_yaml::from_str(&policy_content)
                    .with_context(|| format!("failed to parse policy: {}", path))?;

                let mut resolved = match crate::model::parse_expected_entry(&value) {
                    Ok(expected) => expected,
                    Err(parse_error) => anyhow::bail!(
                        "failed to resolve expected reference '{}': {}. A referenced args_valid policy must be an Expected block or a tool-name-to-schema map under type: args_valid",
                        path,
                        parse_error
                    ),
                };
                resolve_nested_expected_paths(&mut resolved, &reference_path);
                test.expected = resolved;
            }
            _ => {}
        }

        crate::model::validate_test_case_for_execution(test)
            .with_context(|| format!("test '{}': resolved expected block is invalid", test.id))?;
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

fn resolve_nested_expected_paths(expected: &mut Expected, reference_path: &Path) {
    let base = reference_path.parent().unwrap_or(Path::new("."));
    let resolve = |path: &mut String| {
        let candidate = Path::new(path);
        if !candidate.is_absolute() {
            *path = base.join(candidate).to_string_lossy().into_owned();
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

fn read_policy_file(base_dir: &Path, policy_rel: &str) -> Result<String> {
    let path = base_dir.join(policy_rel);
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read policy file: {}", path.display()))
}
