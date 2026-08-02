use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum UnconstrainedMode {
    Warn,
    Deny,
    Allow,
}

#[derive(Debug, Clone)]
pub(super) struct StructuredPolicy {
    pub(super) allow: Vec<String>,
    pub(super) deny: Vec<String>,
    pub(super) schemas: HashMap<String, serde_json::Value>,
    pub(super) unconstrained: UnconstrainedMode,
}

#[derive(Debug, Clone)]
pub(super) enum PolicySource {
    SchemaMap(HashMap<String, serde_json::Value>),
    Structured(StructuredPolicy),
}

fn extract_string_list(val: Option<&serde_json::Value>) -> Vec<String> {
    val.and_then(|v| v.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(ToString::to_string))
                .collect()
        })
        .unwrap_or_default()
}

fn parse_unconstrained_mode(policy_json: &serde_json::Value) -> anyhow::Result<UnconstrainedMode> {
    match policy_json
        .pointer("/enforcement/unconstrained_tools")
        .and_then(|v| v.as_str())
    {
        Some("deny") => Ok(UnconstrainedMode::Deny),
        Some("allow") => Ok(UnconstrainedMode::Allow),
        Some("warn") | None => Ok(UnconstrainedMode::Warn),
        Some(_) => anyhow::bail!(
            "config error: enforcement.unconstrained_tools must be one of: warn, deny, allow"
        ),
    }
}

fn has_structured_policy_shape(root: &serde_json::Value) -> bool {
    let Some(root) = root.as_object() else {
        return false;
    };
    root.get("version")
        .is_some_and(serde_json::Value::is_string)
        || root.get("name").is_some_and(serde_json::Value::is_string)
        || root.get("allow").is_some_and(serde_json::Value::is_array)
        || root.get("deny").is_some_and(serde_json::Value::is_array)
        || root
            .get("tools")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|tools| tools.contains_key("allow") || tools.contains_key("deny"))
        || root
            .get("enforcement")
            .and_then(serde_json::Value::as_object)
            .is_some_and(|enforcement| enforcement.contains_key("unconstrained_tools"))
        || [
            "constraints",
            "limits",
            "signatures",
            "tool_pins",
            "discovery",
            "runtime_monitor",
            "kill_switch",
        ]
        .iter()
        .any(|key| root.contains_key(*key))
        || root.get("schemas").is_some_and(|schemas| {
            schemas.as_object().is_none_or(|schemas| {
                !schemas.is_empty()
                    && schemas
                        .values()
                        .all(|schema| schema.is_object() || schema.is_boolean())
            })
        })
}

pub(super) fn load_policy_source(path: &Path) -> anyhow::Result<PolicySource> {
    let policy_content = std::fs::read_to_string(path).map_err(|e| {
        anyhow::anyhow!(
            "config error: failed to read args_valid policy '{}': {}",
            path.display(),
            e
        )
    })?;

    let policy_json: serde_json::Value = serde_yaml::from_str(&policy_content)
        .map_err(|e| anyhow::anyhow!("config error: invalid args_valid policy YAML: {}", e))?;

    load_policy_source_value(policy_json)
}

pub(super) fn load_policy_source_value(
    policy_json: serde_json::Value,
) -> anyhow::Result<PolicySource> {
    if has_structured_policy_shape(&policy_json) {
        let allow = {
            let mut merged = extract_string_list(policy_json.get("allow"));
            merged.extend(extract_string_list(policy_json.pointer("/tools/allow")));
            merged
        };
        let deny = {
            let mut merged = extract_string_list(policy_json.get("deny"));
            merged.extend(extract_string_list(policy_json.pointer("/tools/deny")));
            merged
        };
        let schemas = policy_json
            .get("schemas")
            .and_then(|v| v.as_object())
            .map(|m| {
                m.iter()
                    .map(|(k, v)| (k.clone(), v.clone()))
                    .collect::<HashMap<String, serde_json::Value>>()
            })
            .unwrap_or_default();

        Ok(PolicySource::Structured(StructuredPolicy {
            allow,
            deny,
            schemas,
            unconstrained: parse_unconstrained_mode(&policy_json)?,
        }))
    } else {
        let schemas: HashMap<String, serde_json::Value> = serde_json::from_value(policy_json)
            .map_err(|e| anyhow::anyhow!("config error: invalid args_valid schema map: {}", e))?;
        Ok(PolicySource::SchemaMap(schemas))
    }
}
