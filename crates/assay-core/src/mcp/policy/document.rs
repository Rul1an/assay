//! The policy file, before defaults and legacy constraints are normalized for enforcement.
use super::deserialize::{normalize_constraints, LegacyConstraints};
use super::types::*;
use super::{McpPolicyError, McpPolicyErrorKind};
use crate::mcp::{identity::ToolIdentity, tool_taxonomy::ToolTaxonomy};
use assay_common::closed;
use assay_common::doc::{load, load_with_checked_source, load_with_string_keys, Opaque, Refusal};
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
            }
        }
        closed! {
            root(yaml_types, x_extensions) struct MigrationDocument {
                $( $(#[$($attr)*])* $field: $ty, )*
                #[default]
                tools: DocumentTools,
                allow: Option<Vec<String>>,
                deny: Option<Vec<String>>,
                constraints: Option<LegacyConstraints>,
            }
        }

        impl MigrationDocument {
            fn normalized(self, upgrade_version: bool) -> anyhow::Result<(McpPolicy, bool)> {
                let Self { $( $field, )* tools, allow, deny, constraints } = self;
                let mut policy = McpPolicy::from(PolicyDocument { $( $field, )* tools });
                for (current, legacy) in [(&mut policy.tools.allow, allow), (&mut policy.tools.deny, deny)] {
                    if let Some(legacy) = legacy {
                        current.get_or_insert_with(Vec::new).extend(legacy);
                    }
                }
                let rules = normalize_constraints(constraints);
                let has_constraints = !rules.is_empty();
                if upgrade_version || has_constraints {
                    super::schema::migrate_constraints_to_schemas(&mut policy, rules);
                }
                Ok((super::legacy::finish(policy)?, has_constraints))
            }
        }

        closed! {
            root(x_extensions) struct CoverageDocument {
                $( $(#[$($attr)*])* $field: $ty, )*
                #[default]
                tools: CoverageTools,
                metadata: Option<Opaque<serde_json::Value>>,
                on_error: Option<crate::on_error::ErrorPolicy>,
            }
        }

        closed! {
            root(x_extensions) struct CoverageMigrationDocument {
                $( $(#[$($attr)*])* $field: $ty, )*
                #[default]
                tools: CoverageTools,
                metadata: Option<Opaque<serde_json::Value>>,
                on_error: Option<crate::on_error::ErrorPolicy>,
                allow: Option<Vec<String>>,
                deny: Option<Vec<String>>,
                constraints: Option<LegacyConstraints>,
            }
        }

        impl CoverageMigrationDocument {
            fn split(self) -> (MigrationDocument, CoverageDocument) {
                let Self { $( $field, )* tools, metadata, on_error, allow, deny, constraints } = self;
                let expected = CoverageDocument { $( $field: $field.clone(), )* tools: tools.clone(), metadata, on_error };
                let (tools, _) = tools.split();
                (MigrationDocument { $( $field, )* tools, allow, deny, constraints }, expected)
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

            fn into_policies(self) -> Result<(McpPolicy, crate::model::Policy), McpPolicyError> {
                let Self { $( $field, )* tools, metadata, on_error } = self;
                let (tools, mut coverage_tools) = tools.split();
                let mut document = PolicyDocument { $( $field, )* tools };
                let sequences = document.sequences.take().unwrap_or_default();
                let aliases = document.aliases.take().unwrap_or_default();
                let enforcement = document.into_current()?;
                coverage_tools.allow = enforcement.tools.allow.clone();
                coverage_tools.deny = enforcement.tools.deny.clone();
                let coverage = crate::model::Policy {
                    version: enforcement.version.clone(),
                    name: enforcement.name.clone(),
                    tools: coverage_tools,
                    sequences,
                    aliases,
                    metadata: metadata.map(|v| v.0),
                    on_error: on_error.unwrap_or_default(),
                };
                Ok((enforcement, coverage))
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

// Rewriters must retain fields owned by other consumers and root extension keys.
fn read_for_rewrite<T: assay_common::doc::ClosedDoc>(
    bytes: &[u8],
) -> Result<(T, serde_yaml::Value), McpPolicyError> {
    let input = assay_common::doc::Input::parse_bytes(bytes).map_err(refusal)?;
    let source = input.source().clone();
    Ok((input.read().map_err(refusal)?, source))
}

impl McpPolicy {
    /// Normalize a current policy's YAML while preserving its authored fields.
    pub fn format_yaml(bytes: &[u8]) -> anyhow::Result<String> {
        let (document, source) = read_for_rewrite::<PolicyDocument>(bytes)?;
        super::legacy::finish(document.into_current()?)?;
        Ok(serde_yaml::to_string(&source)?)
    }

    /// Verify the semantic digest of retained policy bytes using the historical file form.
    /// This returns only an identity; restoring or activating those bytes requires a current policy.
    pub fn retained_policy_digest(bytes: &[u8]) -> anyhow::Result<String> {
        let document = assay_common::doc::Input::parse_bytes(bytes)
            .map_err(refusal)?
            .read::<MigrationDocument>()
            .map_err(refusal)?;
        let (policy, _) = document.normalized(false)?;
        policy
            .try_compile_all_schemas()
            .map_err(|error| anyhow::anyhow!("policy schemas failed to compile: {error}"))?;
        policy
            .policy_digest()
            .ok_or_else(|| anyhow::anyhow!("failed to canonicalize policy digest"))
    }

    /// Convert legacy enforcement fields while retaining fields read by other policy consumers.
    pub fn migrate_yaml(bytes: &[u8]) -> anyhow::Result<String> {
        Self::rewrite_migration(bytes, false)
    }

    /// Migrate a coverage policy using the coverage reader's text scalar semantics.
    pub fn migrate_coverage_yaml(bytes: &[u8]) -> anyhow::Result<String> {
        Self::rewrite_migration(bytes, true)
    }

    fn rewrite_migration(bytes: &[u8], coverage: bool) -> anyhow::Result<String> {
        let input = assay_common::doc::Input::parse_bytes(bytes).map_err(refusal)?;
        let mut source = input.source().clone();
        let (document, expected) = if coverage {
            let (document, expected) = input
                .read::<CoverageMigrationDocument>()
                .map_err(refusal)?
                .split();
            (document, Some(expected))
        } else {
            (input.read::<MigrationDocument>().map_err(refusal)?, None)
        };
        let (policy, has_constraints) = document.normalized(true)?;
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
        Ok(document.into_policies()?)
    }
}

pub(super) fn read(text: &str) -> Result<McpPolicy, McpPolicyError> {
    load::<PolicyDocument>(text)
        .map_err(refusal)?
        .into_current()
}

pub(super) fn read_with_source(
    text: &str,
) -> Result<(McpPolicy, super::PolicySource), McpPolicyError> {
    let (document, source) = load_with_checked_source::<PolicyDocument>(text).map_err(refusal)?;
    Ok((document.into_current()?, super::PolicySource(source)))
}

impl PolicyDocument {
    fn into_current(self) -> Result<McpPolicy, McpPolicyError> {
        if self.version == "1.0" {
            return Err(McpPolicyError {
                kind: McpPolicyErrorKind::Structure,
                source: anyhow::anyhow!(
                    "policy version 1.0 requires conversion with assay policy migrate"
                ),
            });
        }
        Ok(McpPolicy::from(self))
    }
}

impl<'de> serde::Deserialize<'de> for McpPolicy {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let tree = serde_yaml::Value::deserialize(deserializer)?;
        assay_common::doc::read_yaml_tree::<PolicyDocument>(tree)
            .map_err(refusal)
            .and_then(PolicyDocument::into_current)
            .map_err(serde::de::Error::custom)
    }
}

impl From<PolicyDocument> for McpPolicy {
    fn from(document: PolicyDocument) -> Self {
        let PolicyDocument {
            version,
            name,
            tools,
            schemas,
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
            schemas: schemas
                .into_iter()
                .map(|(tool, schema)| (tool, schema.0))
                .collect(),
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
        Refusal::UnknownKeys(keys) if keys.iter().any(|key| matches!(key.as_str(), "allow" | "deny" | "constraints")) => (
            McpPolicyErrorKind::UnknownKeys,
            anyhow::anyhow!("unknown policy keys: {keys:?}; convert legacy root fields with assay policy migrate"),
        ),
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
