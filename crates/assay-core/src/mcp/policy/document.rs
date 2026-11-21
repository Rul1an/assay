//! The policy file, before defaults and legacy constraints are normalized for enforcement.
use super::deserialize::{normalize_constraints, LegacyConstraints};
use super::types::*;
use super::{McpPolicyError, McpPolicyErrorKind};
use crate::mcp::{identity::ToolIdentity, tool_taxonomy::ToolTaxonomy};
use assay_common::closed;
use assay_common::doc::{load, load_with_source, load_with_string_keys, Opaque, Refusal};
use std::collections::{BTreeSet, HashMap};

// The two file forms share the enforcement fields. Coverage also accepts the eval policy
// fields. Keep these out of the MCP root, where they are refused.
macro_rules! policy_documents {
    ($( $(#[$($attr:tt)*])* $field:ident : $ty:ty, )*) => {
        closed! {
            root(yaml_types, x_extensions) struct PolicyDocument {
                $( $(#[$($attr)*])* $field: $ty, )*
                #[default]
                tools: DocumentTools,
                allow: Option<Vec<String>>,
                deny: Option<Vec<String>>,
                constraints: Option<LegacyConstraints>,
            }
        }
        closed! {
            root(x_extensions) struct CoverageDocument {
                $( $(#[$($attr)*])* $field: $ty, )*
                #[default]
                tools: CoverageTools,
                allow: Option<Vec<String>>,
                deny: Option<Vec<String>>,
                constraints: Option<LegacyConstraints>,
                metadata: Option<Opaque<serde_json::Value>>,
                on_error: Option<crate::on_error::ErrorPolicy>,
            }
        }

        impl CoverageDocument {
            // Re-emit fields whose text interpretation changed when the source YAML tree
            // was serialized (for example the argument name 0x10 becoming the string "16").
            fn restore_text(&self, actual: &Self, root: &mut serde_yaml::Value) -> anyhow::Result<()> {
                $( restore_field(root, stringify!($field), &self.$field, &actual.$field)?; )*
                restore_field(root, "metadata", &self.metadata, &actual.metadata)?;
                restore_field(root, "on_error", &self.on_error, &actual.on_error)?;
                if self.tools != actual.tools {
                    let typed = ordered_field(&self.tools)?;
                    let observed = serde_yaml::to_value(&actual.tools)?;
                    for (key, value) in root["tools"].as_mapping_mut()
                        .ok_or_else(|| anyhow::anyhow!("coverage tools must be a mapping"))? {
                        if typed.get(key) != observed.get(key) {
                            *value = typed.get(key).cloned()
                                .ok_or_else(|| anyhow::anyhow!("unrecognized coverage tool field"))?;
                        }
                    }
                }
                Ok(())
            }

            fn raw_policies(self) -> (McpPolicy, crate::model::Policy) {
                let Self { $( $field, )* tools, metadata, on_error, allow, deny, constraints } = self;
                let (tools, coverage_tools) = tools.split();
                let mut document = PolicyDocument { $( $field, )* tools, allow, deny, constraints };
                let coverage = crate::model::Policy {
                    version: document.version.clone(),
                    name: document.name.clone(),
                    tools: coverage_tools,
                    sequences: document.sequences.take().unwrap_or_default(),
                    aliases: document.aliases.take().unwrap_or_default(),
                    metadata: metadata.map(|v| v.0),
                    on_error: on_error.unwrap_or_default(),
                };
                (McpPolicy::from(document), coverage)
            }

            fn into_policies(self) -> (McpPolicy, crate::model::Policy) {
                let (mut enforcement, mut coverage) = self.raw_policies();
                enforcement.normalize_legacy_shapes();
                if !enforcement.constraints.is_empty() {
                    enforcement.migrate_constraints_to_schemas();
                }
                coverage.version = enforcement.version.clone();
                coverage.tools.allow = enforcement.tools.allow.clone();
                coverage.tools.deny = enforcement.tools.deny.clone();
                (enforcement, coverage)
            }
        }
    };
}

policy_documents! {
    #[default]
    version: String,
    #[default]
    name: String,
    #[default]
    schemas: HashMap<String, Opaque<serde_json::Value>>,
    #[default]
    enforcement: EnforcementSettings,
    limits: Option<GlobalLimits>,
    signatures: Option<SignaturePolicy>,
    #[default]
    tool_pins: HashMap<String, ToolIdentity>,
    #[default]
    tool_classes: HashMap<String, BTreeSet<String>>,
    sequences: Option<Vec<crate::model::SequenceRule>>,
    aliases: Option<HashMap<String, Vec<String>>>,
    /// Read by assay_policy_decide.
    blocklist: Option<Vec<String>>,
    discovery: Option<DiscoveryConfig>,
    runtime_monitor: Option<RuntimeMonitorConfig>,
    kill_switch: Option<KillSwitchConfig>,
}

// Restored fields contain typed string-keyed maps. HashMap iteration order must
// not change preview bytes across processes. Leave authored source maps alone.
fn ordered_field<T: serde::Serialize>(field: &T) -> anyhow::Result<serde_yaml::Value> {
    let mut value = serde_json::to_value(field)?;
    value.sort_all_objects();
    Ok(serde_yaml::to_value(value)?)
}

fn restore_field<T: serde::Serialize + PartialEq>(
    root: &mut serde_yaml::Value,
    key: &str,
    expected: &T,
    actual: &T,
) -> anyhow::Result<()> {
    if expected != actual {
        root[key] = ordered_field(expected)?;
    }
    Ok(())
}

// Coverage decodes struct field names from YAML text, so a tagged string names
// the same field as its untagged spelling. Only normalize keys the writer owns.
fn untag_field_names(map: &mut serde_yaml::Mapping) -> anyhow::Result<()> {
    if map
        .keys()
        .any(|key| matches!(key, serde_yaml::Value::Tagged(_)))
    {
        for (key, value) in std::mem::take(map) {
            let key = match key.as_str() {
                Some(name) if !assay_common::doc::is_extension_key(name) => {
                    serde_yaml::Value::String(name.to_owned())
                }
                _ => key,
            };
            anyhow::ensure!(map.insert(key, value).is_none(), "duplicate coverage field");
        }
    }
    Ok(())
}

impl McpPolicy {
    /// Convert legacy enforcement fields while retaining fields read by other policy consumers.
    pub fn migrate_yaml(bytes: &[u8]) -> anyhow::Result<String> {
        Self::rewrite_migration(bytes, false)
    }

    /// Migrate a coverage policy using the coverage reader's text scalar semantics.
    pub fn migrate_coverage_yaml(bytes: &[u8]) -> anyhow::Result<String> {
        Self::rewrite_migration(bytes, true)
    }

    fn rewrite_migration(bytes: &[u8], coverage: bool) -> anyhow::Result<String> {
        let text = super::legacy::policy_text(bytes)?;
        let mut source = super::legacy::yaml_value(bytes)?;
        let (policy, expected) = if coverage {
            let document = load_with_string_keys::<CoverageDocument>(text).map_err(refusal)?;
            let (policy, _) = document.clone().raw_policies();
            (policy, Some(document))
        } else {
            (read(text)?, None)
        };
        let has_constraints = !policy.constraints.is_empty();
        let mut policy = super::legacy::finish(policy)?;
        policy.migrate_constraints_to_schemas();
        policy
            .try_compile_all_schemas()
            .map_err(|e| anyhow::anyhow!("migrated policy schemas failed to compile: {e}"))?;

        if source.is_null() {
            source = serde_yaml::Value::Mapping(Default::default());
        }
        let root = source
            .as_mapping_mut()
            .ok_or_else(|| anyhow::anyhow!("policy root must be a mapping"))?;
        if coverage {
            untag_field_names(root)?;
            if let Some(tools) = root
                .get_mut("tools")
                .and_then(serde_yaml::Value::as_mapping_mut)
            {
                untag_field_names(tools)?;
            }
        }
        root.insert("version".into(), policy.version.clone().into());
        if coverage && root.contains_key("name") {
            root.insert("name".into(), policy.name.clone().into());
        }
        for (key, values) in [("allow", &policy.tools.allow), ("deny", &policy.tools.deny)] {
            let legacy = root.remove(key).is_some_and(|value| !value.is_null());
            if legacy || (coverage && values.is_some()) {
                let tools = root
                    .entry("tools".into())
                    .or_insert(serde_yaml::Value::Null);
                if tools.is_null() {
                    *tools = serde_yaml::Value::Mapping(Default::default());
                }
                tools
                    .as_mapping_mut()
                    .ok_or_else(|| anyhow::anyhow!("policy tools must be a mapping"))?
                    .insert(key.into(), serde_yaml::to_value(values)?);
            }
        }
        root.remove("constraints");
        if has_constraints {
            // Schema map keys are decoded as tool names. Write the effective map so a tagged
            // spelling of a migrated name cannot shadow its replacement when the file reloads.
            let schemas: std::collections::BTreeMap<_, _> = policy.schemas.iter().collect();
            root.insert("schemas".into(), serde_yaml::to_value(schemas)?);
        }
        let mut yaml = serde_yaml::to_string(&source)?;
        if let Some(mut expected) = expected {
            expected.allow = None;
            expected.deny = None;
            expected.constraints = None;
            expected.version = policy.version;
            expected.tools.allow = policy.tools.allow;
            expected.tools.deny = policy.tools.deny;
            if has_constraints {
                expected.schemas = policy
                    .schemas
                    .into_iter()
                    .map(|(k, v)| (k, Opaque(v)))
                    .collect();
            }
            let reloaded = load_with_string_keys::<CoverageDocument>(&yaml).map_err(refusal)?;
            if reloaded != expected {
                expected.restore_text(&reloaded, &mut source)?;
                yaml = serde_yaml::to_string(&source)?;
                anyhow::ensure!(
                    load_with_string_keys::<CoverageDocument>(&yaml).map_err(refusal)? == expected,
                    "migration would change coverage policy fields"
                );
            }
            anyhow::ensure!(
                yaml.len() as u64 <= crate::model::MAX_POLICY_INPUT_BYTES,
                "converted policy exceeds the {}-byte coverage input limit",
                crate::model::MAX_POLICY_INPUT_BYTES
            );
        }
        Ok(yaml)
    }

    /// Reads the enforcement and coverage views of the same coverage policy document.
    pub fn load_for_coverage(text: &str) -> anyhow::Result<(Self, crate::model::Policy)> {
        let document = load_with_string_keys::<CoverageDocument>(text).map_err(refusal)?;
        Ok(document.into_policies())
    }
}

pub(super) fn read(text: &str) -> Result<McpPolicy, McpPolicyError> {
    load::<PolicyDocument>(text)
        .map(McpPolicy::from)
        .map_err(refusal)
}

pub(super) fn read_with_source(
    text: &str,
) -> Result<(McpPolicy, super::PolicySource), McpPolicyError> {
    let (document, source) = load_with_source::<PolicyDocument>(text).map_err(refusal)?;
    Ok((McpPolicy::from(document), super::PolicySource(source)))
}

impl From<PolicyDocument> for McpPolicy {
    fn from(document: PolicyDocument) -> Self {
        let PolicyDocument {
            version,
            name,
            tools,
            allow,
            deny,
            schemas,
            constraints,
            enforcement,
            limits,
            signatures,
            tool_pins,
            tool_classes,
            sequences: _,
            blocklist: _,
            aliases: _,
            discovery,
            runtime_monitor,
            kill_switch,
        } = document;
        McpPolicy {
            version,
            name,
            tools: tools.into(),
            allow,
            deny,
            schemas: schemas
                .into_iter()
                .map(|(tool, schema)| (tool, schema.0))
                .collect(),
            constraints: normalize_constraints(constraints),
            enforcement,
            limits,
            signatures,
            tool_pins,
            tool_taxonomy: ToolTaxonomy { tool_classes },
            discovery,
            runtime_monitor,
            kill_switch,
            compiled: Default::default(),
        }
    }
}

fn refusal(error: Refusal) -> McpPolicyError {
    use McpPolicyErrorKind::{RootNotMapping, Structure, Syntax};
    let (kind, source) = match error {
        Refusal::Syntax {
            line,
            column,
            message,
        } => (Syntax { line, column }, anyhow::anyhow!("{message}")),
        Refusal::UnknownKeys(keys) => (
            McpPolicyErrorKind::UnknownKeys,
            anyhow::anyhow!("unknown policy keys: {keys:?}"),
        ),
        Refusal::NotAMapping { path, .. } if path.is_empty() => (
            RootNotMapping,
            anyhow::anyhow!("policy root is not a YAML mapping"),
        ),
        Refusal::NotAMapping { path, .. } => (
            Structure,
            anyhow::anyhow!("expected a mapping at policy key {path:?}"),
        ),
        Refusal::Shape { message, .. } => (Structure, anyhow::anyhow!("{message}")),
        _ => (Structure, anyhow::anyhow!("invalid policy document")),
    };
    McpPolicyError { kind, source }
}
