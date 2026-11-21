//! The file accepted by the eval policy model and by `assay explain`.
use super::{Policy, SequenceRule, ToolsPolicy};
use crate::on_error::ErrorPolicy;
use anyhow::Context;
use assay_common::doc::{Input, Opaque, Refusal};
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

/// Read local policy text with the same source limit as policy validation.
pub fn read_policy_text(path: &Path) -> std::io::Result<String> {
    bounded_text(&File::open(path)?)
}

fn bounded_text(file: &File) -> std::io::Result<String> {
    let mut text = String::new();
    LimitReader::new(file, MAX_POLICY_INPUT_BYTES, LimitKind::SourceBytes)
        .read_to_string(&mut text)?;
    Ok(text)
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
        let text = bounded_text(&file).with_context(read_error)?;

        let policy = assay_common::doc::load_with_string_keys::<PolicyFile>(&text)
            .map(Self::from)
            .map_err(|err| positional_policy_diagnostic(&text, err))
            .map_err(policy_refusal)
            .with_context(|| format!("Failed to parse policy: {}", path.display()))?;
        Ok((policy, file))
    }

    /// Decode bounded MCP tool input, which requires an untagged mapping root.
    #[doc(hidden)]
    pub fn from_tool_text(text: &str) -> Result<Self, Refusal> {
        assay_common::doc::load_bare_mapping_with_string_keys::<PolicyFile>(text).map(Self::from)
    }

    /// Decode a previously admitted policy document, retaining typed refusal details.
    #[doc(hidden)]
    pub fn from_document(input: Input<'_>) -> Result<Self, Refusal> {
        input.read::<PolicyFile>().map(Self::from)
    }
}

impl From<PolicyFile> for Policy {
    fn from(doc: PolicyFile) -> Self {
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
        }
    }
}

// In a positional sequence rule serde buffers the selector before reporting its
// error. Recover the original field path when the YAML shape identifies it.
fn positional_policy_diagnostic(text: &str, error: Refusal) -> Refusal {
    if !matches!(&error, Refusal::Shape { message, .. } if message.contains("unknown field")) {
        return error;
    }
    match Input::parse(text).and_then(Policy::from_document) {
        Err(Refusal::UnknownKeys(keys)) if keys.iter().all(|key| key.starts_with("sequences.")) => {
            Refusal::UnknownKeys(keys)
        }
        _ => error,
    }
}

pub(super) fn policy_refusal(err: Refusal) -> anyhow::Error {
    match err {
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
    }
}

/// The three on-disk forms accepted before the ADR-052 major cut.
#[derive(Clone, Debug)]
pub enum SequenceDocument {
    Exact(Vec<String>),
    Rules(Vec<SequenceRule>),
    Policy(Box<Policy>),
}

impl SequenceDocument {
    pub fn parse(source: &str) -> anyhow::Result<Self> {
        Self::read(assay_common::doc::Input::parse(source).map_err(policy_refusal)?)
    }

    pub(super) fn read(input: assay_common::doc::Input<'_>) -> anyhow::Result<Self> {
        if input.source().is_mapping() {
            return Policy::from_document(input)
                .map(|policy| Self::Policy(Box::new(policy)))
                .map_err(policy_refusal);
        }
        // An empty list is an exact sequence of zero calls. Only a list of mappings
        // is a rule list. Decode names from the original text to keep scalar spelling.
        if input
            .source()
            .as_sequence()
            .is_some_and(|items| items.first().is_some_and(|item| item.is_mapping()))
        {
            input.read().map(Self::Rules).map_err(policy_refusal)
        } else {
            input.read().map(Self::Exact).map_err(policy_refusal)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::unread_keys::keys_of;

    #[test]
    fn sequence_forms_keep_scalar_spellings_and_refuse_mixed_lists() {
        let SequenceDocument::Exact(names) =
            SequenceDocument::parse("!list [01, TRUE, read]").unwrap()
        else {
            panic!("expected exact tool names")
        };
        assert_eq!(names, ["01", "TRUE", "read"]);
        assert!(matches!(
            SequenceDocument::parse("[{type: require, tool: read}]").unwrap(),
            SequenceDocument::Rules(rules) if rules.len() == 1
        ));
        for text in [
            "null",
            "~",
            "[read, {type: require, tool: read}]",
            "[{type: require, tool: read}, read]",
        ] {
            assert!(SequenceDocument::parse(text).is_err(), "{text:?}");
        }
    }

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
