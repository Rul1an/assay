use crate::model::{EvalConfig, Expected};
use anyhow::{Context, Result};
use std::path::Path;

pub fn resolve_policies(mut config: EvalConfig, base_dir: &Path) -> Result<EvalConfig> {
    for test in &mut config.tests {
        match &mut test.expected {
            Expected::ArgsValid {
                ref mut policy,
                ref mut schema,
            } => {
                if schema.is_none() {
                    if let Some(path) = policy {
                        let policy_content = read_policy_file(base_dir, path)?;
                        let loaded: serde_json::Value = serde_yaml::from_str(&policy_content)
                            .with_context(|| format!("failed to parse policy YAML: {}", path))?;

                        *schema = Some(loaded);
                        *policy = None;
                    }
                }
            }
            Expected::SequenceValid {
                ref mut policy,
                ref mut sequence,
                ref mut rules,
            } => {
                if sequence.is_none() && rules.is_none() {
                    if let Some(path) = policy {
                        let policy_content = read_policy_file(base_dir, path)?;

                        // Try parsing as simple sequence first, then rules
                        if let Ok(loaded) = serde_yaml::from_str::<Vec<String>>(&policy_content) {
                            *sequence = Some(loaded);
                        } else if let Ok(loaded) =
                            serde_yaml::from_str::<Vec<crate::model::SequenceRule>>(&policy_content)
                        {
                            *rules = Some(loaded);
                        } else {
                            anyhow::bail!("Failed to parse sequence policy '{}' as either list of strings or rules", path);
                        }

                        *policy = None;
                    }
                }
            }
            _ => {}
        }
    }

    // Auto-bump version if resolving?
    // The user plan says: "Dit doet dezelfde path→inline transformatie".
    // It doesn't explicitly say it bumps version, but keeping it consistent with migration is good.
    // However, for "mixed mode" support, we might just resolved policies without enforcing version=1?
    // User request: "Precedence rule: als configVersion: 1 en schema/rules/blocklist aanwezig → gebruik inline... Als configVersion: 1 maar alleen policy aanwezig → toegestaan"
    // So current load logic handles precedence (by checking fields).
    // `resolve_policies` transforms config to have inline fields.

    // Let's NOT bump version automatically here, let the caller decide (migration command bumps it).
    // But for equivalence tests we want them equal.

    Ok(config)
}

fn read_policy_file(base_dir: &Path, policy_rel: &str) -> Result<String> {
    let path = base_dir.join(policy_rel);
    std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read policy file: {}", path.display()))
}
