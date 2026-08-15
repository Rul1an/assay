use super::{McpPolicy, McpPolicyError, McpPolicyErrorKind};
use std::path::Path;
use std::sync::OnceLock;

pub(super) fn from_file(path: &Path) -> anyhow::Result<McpPolicy> {
    let bytes = std::fs::read(path)?;
    McpPolicy::from_slice(&bytes)
}

pub(super) fn from_slice(bytes: &[u8]) -> anyhow::Result<McpPolicy> {
    // UTF-8 decode — failure is Syntax, not an I/O error.
    let content = match std::str::from_utf8(bytes) {
        Ok(s) => s,
        Err(e) => {
            return Err(McpPolicyError {
                kind: McpPolicyErrorKind::Syntax {
                    line: None,
                    column: None,
                },
                source: anyhow::Error::new(e),
            }
            .into());
        }
    };

    // YAML decode to Value — failure is Syntax with location.
    let value: serde_yaml::Value = match serde_yaml::from_str(content) {
        Ok(v) => v,
        Err(e) => {
            let loc = e.location();
            return Err(McpPolicyError {
                kind: McpPolicyErrorKind::Syntax {
                    line: loc.as_ref().map(|l| l.line()),
                    column: loc.as_ref().map(|l| l.column()),
                },
                source: anyhow::Error::new(e),
            }
            .into());
        }
    };

    // Root must be a mapping.
    if !value.is_mapping() {
        return Err(McpPolicyError {
            kind: McpPolicyErrorKind::RootNotMapping,
            source: anyhow::anyhow!("policy root is not a YAML mapping"),
        }
        .into());
    }

    // Typed deserialization with unknown-field tracking.
    let mut unknown = Vec::new();
    let mut policy: McpPolicy = match serde_ignored::deserialize(value, |path| {
        unknown.push(path.to_string());
    }) {
        Ok(p) => p,
        Err(e) => {
            return Err(McpPolicyError {
                kind: McpPolicyErrorKind::Structure,
                source: anyhow::Error::new(e),
            }
            .into());
        }
    };

    if !unknown.is_empty() {
        tracing::warn!(?unknown, "Unknown fields in policy (ignored)");
    }

    // Check for v1 format and warn if necessary
    if is_v1_format(&policy) {
        if std::env::var("ASSAY_STRICT_DEPRECATIONS").ok().as_deref() == Some("1") {
            anyhow::bail!("Strict mode: v1 policy format (constraints) is not allowed.");
        }
        emit_deprecation_warning();
    }

    // Normalize legacy shapes
    normalize_legacy_shapes(&mut policy);

    // Auto-migrate v1 constraints
    if !policy.constraints.is_empty() {
        policy.migrate_constraints_to_schemas();
    }

    // Validation — failure is Validation kind.
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

pub(super) fn is_v1_format(policy: &McpPolicy) -> bool {
    // v1 if constraints are present OR version is explicitly "1.0"
    !policy.constraints.is_empty() || policy.version == "1.0"
}

pub(super) fn normalize_legacy_shapes(policy: &mut McpPolicy) {
    if let Some(allow) = policy.allow.take() {
        let mut current = policy.tools.allow.take().unwrap_or_default();
        current.extend(allow);
        policy.tools.allow = Some(current);
    }
    if let Some(deny) = policy.deny.take() {
        let mut current = policy.tools.deny.take().unwrap_or_default();
        current.extend(deny);
        policy.tools.deny = Some(current);
    }
}

fn emit_deprecation_warning() {
    static WARNED: OnceLock<()> = OnceLock::new();
    WARNED.get_or_init(|| {
        eprintln!(
            "\n\x1b[33m⚠️  DEPRECATED: v1 policy format detected\x1b[0m\n\
             \x1b[33m   The 'constraints:' syntax is deprecated and will be removed in Assay v2.0.0.\x1b[0m\n\
             \x1b[33m   Migrate now:\x1b[0m\n\
             \x1b[33m     assay policy migrate --input <file>\x1b[0m\n\
             \x1b[33m   See: https://docs.getassay.dev/migration/v1-to-v2\x1b[0m\n"
        );
    });
}
