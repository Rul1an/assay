//! The file accepted by the eval policy model and by `assay explain`.
use super::{Policy, SequenceRule, ToolsPolicy};
use crate::on_error::ErrorPolicy;
use anyhow::Context;
use assay_common::doc::{load_with_string_keys, Opaque, Refusal};
use assay_common::limits::{LimitKind, LimitReader};
use serde_json::Value;
use std::{collections::HashMap, fs::File, io::Read, path::Path};

pub const MAX_POLICY_INPUT_BYTES: u64 = 1_000_000;

assay_common::closed! {
    root struct PolicyFile {
        version: String,
        #[default]
        name: String,
        metadata: Option<Opaque<Value>>,
        #[default]
        tools: FileTools,
        #[default]
        sequences: Vec<SequenceRule>,
        #[default]
        aliases: HashMap<String, Vec<String>>,
        #[default]
        on_error: ErrorPolicy,
    }
}

assay_common::closed! {
    #[derive(Default)]
    struct FileTools {
        allow: Option<Vec<String>>,
        deny: Option<Vec<String>>,
        require_args: Option<HashMap<String, Vec<String>>>,
        arg_constraints: Option<Opaque<HashMap<String, HashMap<String, Value>>>>,
    }
}

impl Policy {
    pub fn load<P: AsRef<Path>>(path: P) -> anyhow::Result<Self> {
        let path = path.as_ref();
        Self::load_with_file(path).map(|(policy, _)| policy)
    }

    /// Keeps the original file handle for callers that guard output against input aliases.
    pub fn load_with_file(path: &Path) -> anyhow::Result<(Self, File)> {
        let read_error = || format!("Failed to read policy: {}", path.display());
        let file = File::open(path).with_context(read_error)?;
        let mut text = String::new();
        LimitReader::new(&file, MAX_POLICY_INPUT_BYTES, LimitKind::SourceBytes)
            .read_to_string(&mut text)
            .with_context(read_error)?;

        let doc = load_with_string_keys::<PolicyFile>(&text)
            .map_err(|err| match err {
                Refusal::UnknownKeys(keys) => anyhow::anyhow!("unknown policy keys: {keys:?}"),
                Refusal::NotAMapping { path, .. } if path.is_empty() => {
                    anyhow::anyhow!("policy root is not a YAML mapping")
                }
                Refusal::NotAMapping { path, .. } => {
                    anyhow::anyhow!("expected a mapping at policy key {path:?}")
                }
                Refusal::Syntax { message, .. } | Refusal::Shape { message, .. } => {
                    anyhow::anyhow!("{message}")
                }
                _ => anyhow::anyhow!("invalid policy document"),
            })
            .with_context(|| format!("Failed to parse policy: {}", path.display()))?;
        Ok((
            Self {
                version: doc.version,
                name: doc.name,
                metadata: doc.metadata.map(|value| value.0),
                tools: ToolsPolicy {
                    allow: doc.tools.allow,
                    deny: doc.tools.deny,
                    require_args: doc.tools.require_args,
                    arg_constraints: doc.tools.arg_constraints.map(|value| value.0),
                },
                sequences: doc.sequences,
                aliases: doc.aliases,
                on_error: doc.on_error,
            },
            file,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unread_keys::keys_of;

    #[test]
    fn file_and_public_policy_declarations_have_the_same_keys() {
        assert_eq!(
            keys_of::<PolicyFile>().unwrap(),
            keys_of::<Policy>().unwrap()
        );
        assert_eq!(
            keys_of::<FileTools>().unwrap(),
            keys_of::<ToolsPolicy>().unwrap()
        );
    }
}
