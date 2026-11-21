use super::{McpPolicy, McpPolicyError, McpPolicyErrorKind};
use std::path::Path;

pub(super) fn from_file(path: &Path) -> anyhow::Result<McpPolicy> {
    let bytes = read_file(path)?;
    McpPolicy::from_slice(&bytes)
}

pub(super) fn read_file(path: &Path) -> anyhow::Result<Vec<u8>> {
    std::fs::read(path).map_err(|error| {
        McpPolicyError {
            kind: McpPolicyErrorKind::Io,
            source: anyhow::Error::new(error)
                .context(format!("failed to read policy {}", path.display())),
        }
        .into()
    })
}

/// The raw YAML tree of a policy file, for tests that build a source the policy grammar refuses.
#[cfg(test)]
pub(super) fn yaml_value(bytes: &[u8]) -> anyhow::Result<serde_yaml::Value> {
    let content = policy_text(bytes)?;
    assay_common::yaml_input::parse_tree(content.as_bytes()).map_err(|error| {
        let location = error.location();
        McpPolicyError {
            kind: McpPolicyErrorKind::Syntax {
                line: location.as_ref().map(|loc| loc.line()),
                column: location.as_ref().map(|loc| loc.column()),
            },
            source: anyhow::Error::new(error),
        }
        .into()
    })
}

pub(super) fn from_slice(bytes: &[u8]) -> anyhow::Result<McpPolicy> {
    finish(super::document::read(policy_text(bytes)?)?)
}

pub(super) fn from_slice_with_source(
    bytes: &[u8],
) -> anyhow::Result<(McpPolicy, super::PolicySource)> {
    let (policy, source) = super::document::read_with_source(policy_text(bytes)?)?;
    Ok((finish(policy)?, source))
}

fn policy_text(bytes: &[u8]) -> anyhow::Result<&str> {
    std::str::from_utf8(bytes)
        .map_err(|error| McpPolicyError {
            kind: McpPolicyErrorKind::Syntax {
                line: None,
                column: None,
            },
            source: anyhow::Error::new(error),
        })
        .map_err(Into::into)
}

pub(super) fn finish(policy: McpPolicy) -> anyhow::Result<McpPolicy> {
    if let Err(e) = validate(&policy) {
        return Err(McpPolicyError {
            kind: McpPolicyErrorKind::Validation,
            source: e,
        }
        .into());
    }

    Ok(policy)
}

pub(super) fn validate(policy: &McpPolicy) -> anyhow::Result<()> {
    let mut pin_names: Vec<&String> = policy.tool_pins.keys().collect();
    pin_names.sort();
    for pin_name in pin_names {
        let pin = &policy.tool_pins[pin_name];
        validate_sha256_hex(
            &pin.schema_hash,
            &format!("tool_pins.{pin_name}.schema_hash"),
        )?;
        validate_sha256_hex(&pin.meta_hash, &format!("tool_pins.{pin_name}.meta_hash"))?;
    }

    // Cross-validation: Kill triggers must reference valid rules
    if let (Some(rm), Some(ks)) = (&policy.runtime_monitor, &policy.kill_switch) {
        let rule_ids: std::collections::HashSet<&str> =
            rm.rules.iter().map(|r| r.id.as_str()).collect();

        for t in &ks.triggers {
            if !rule_ids.contains(t.on_rule.as_str()) {
                anyhow::bail!(
                    "kill_switch.triggers references unknown rule id: {}",
                    t.on_rule
                );
            }
        }
    }
    Ok(())
}

fn validate_sha256_hex(value: &str, field: &str) -> anyhow::Result<()> {
    if value.len() == 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
    {
        return Ok(());
    }
    anyhow::bail!("{field} must be exactly 64 lowercase hexadecimal characters")
}
