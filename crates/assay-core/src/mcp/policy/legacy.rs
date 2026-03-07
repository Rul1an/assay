use super::McpPolicy;
use std::path::Path;
use std::sync::OnceLock;

pub(super) fn from_file(path: &Path) -> anyhow::Result<McpPolicy> {
    let content = std::fs::read_to_string(path)?;

    let mut unknown = Vec::new();
    let de = serde_yaml::Deserializer::from_str(&content);
    let mut policy: McpPolicy = serde_ignored::deserialize(de, |path| {
        unknown.push(path.to_string());
    })
    .map_err(anyhow::Error::from)?;

    if !unknown.is_empty() {
        // Filter out transient/internal fields if any. For now, log all.
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

    validate(&policy)?;

    Ok(policy)
}

pub(super) fn validate(policy: &McpPolicy) -> anyhow::Result<()> {
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
             \x1b[33m   See: https://docs.assay.dev/migration/v1-to-v2\x1b[0m\n"
        );
    });
}
