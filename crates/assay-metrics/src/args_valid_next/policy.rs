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

fn parse_unconstrained_mode(policy_json: &serde_json::Value) -> UnconstrainedMode {
    match policy_json
        .pointer("/enforcement/unconstrained_tools")
        .and_then(|v| v.as_str())
    {
        Some("deny") => UnconstrainedMode::Deny,
        Some("allow") => UnconstrainedMode::Allow,
        _ => UnconstrainedMode::Warn,
    }
}

fn has_structured_policy_shape(root: &serde_json::Value) -> bool {
    [
        "version",
        "name",
        "tools",
        "allow",
        "deny",
        "schemas",
        "constraints",
        "enforcement",
        "limits",
        "signatures",
        "tool_pins",
        "discovery",
        "runtime_monitor",
        "kill_switch",
    ]
    .iter()
    .any(|k| root.get(k).is_some())
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
            unconstrained: parse_unconstrained_mode(&policy_json),
        }))
    } else {
        let schemas: HashMap<String, serde_json::Value> = serde_yaml::from_str(&policy_content)
            .map_err(|e| anyhow::anyhow!("config error: invalid args_valid policy YAML: {}", e))?;
        Ok(PolicySource::SchemaMap(schemas))
    }
}
