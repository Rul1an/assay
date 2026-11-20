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

        impl CoverageDocument {
            fn into_policies(self) -> (McpPolicy, crate::model::Policy) {
                let Self { $( $field, )* tools, metadata, on_error } = self;
                let (tools, mut coverage_tools) = tools.split();
                let mut document = PolicyDocument { $( $field, )* tools };
                let sequences = document.sequences.take().unwrap_or_default();
                let aliases = document.aliases.take().unwrap_or_default();
                let mut enforcement = McpPolicy::from(document);
                enforcement.normalize_legacy_shapes();
                if !enforcement.constraints.is_empty() {
                    enforcement.migrate_constraints_to_schemas();
                }
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
    allow: Option<Vec<String>>,
    deny: Option<Vec<String>>,
    #[default]
    schemas: HashMap<String, Opaque<serde_json::Value>>,
    constraints: Option<LegacyConstraints>,
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

impl McpPolicy {
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
