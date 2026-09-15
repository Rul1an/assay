//! CAP-1 normative verification: bounded admission, the pinned JSON Schema, then R0–R8.
//!
//! This is the stage the relying-party gate in the parent module assumes has already run.
//! Order is fixed by SPEC-Incident-Package-v1 §6 and §10 phase 8: strict syntax and resource
//! admission, JSON Schema 2020-12 validation against the schema bytes embedded from
//! [`CAP1_SCHEMA_SOURCE`], the typed shape, then the rules of [`super::rules`] in numeric order,
//! stopping at the first failure. Refusal is the only response to a non-conforming document;
//! nothing is coerced.
//!
//! Diagnostics are structural: a stage, a JSON pointer, a rule id and a static grounds string.
//! No instance value (a key name, a disposition string, a stratum id) is echoed, so a refusal
//! can be logged without disclosing producer text (SPEC §8, diagnostic output).
//!
//! Conformance establishes internal consistency of the document and nothing else (CAP-1 §1.3,
//! §9). It is not producer truth, capture completeness, or a statement about any agent.

use std::fmt;
use std::sync::OnceLock;

use serde_json::Value;

use super::rules::{verify_cap1_rules_with, Cap1NormativeRule};
use super::Cap1Document;
use crate::json_strict::{validate_json_strict, StrictJsonError};

/// The normative CAP-1 JSON Schema, byte-identical to the pinned upstream file.
pub const CAP1_SCHEMA_JSON: &str = include_str!("../../schemas/cap-1/CAP-1.schema.json");

/// SHA-256 of [`CAP1_SCHEMA_JSON`], the `schema_pin` of SPEC-Incident-Package-v1 §6.
pub const CAP1_SCHEMA_SHA256: &str =
    "4453f216089543780bfecc4295cc4a61462fdc585b88d1e35b7d1aba79716b4a";

/// Where the embedded schema bytes come from. An Internet-Draft pin, not an adopted standard.
pub const CAP1_SCHEMA_SOURCE: &str =
    "Certisyn-Inc/certisyn-drafts@0980d3201aa2caab3cbad5c6e9bc99b422370b43 cap-1/src/CAP-1.schema.json";

/// Byte budget charged before any parsing. The selected value can lower the ceiling and never
/// raise it: the effective limit is `min(max_bytes, HARD_MAX_BYTES)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cap1AdmissionLimits {
    pub max_bytes: usize,
}

impl Cap1AdmissionLimits {
    /// SPEC §8 `health_bytes`: a coverage document standing alone is a sidecar of that class.
    pub const HARD_MAX_BYTES: usize = 1_048_576;

    fn effective_max_bytes(self) -> usize {
        self.max_bytes.min(Self::HARD_MAX_BYTES)
    }
}

impl Default for Cap1AdmissionLimits {
    fn default() -> Self {
        Self {
            max_bytes: Self::HARD_MAX_BYTES,
        }
    }
}

/// Which stage refused. Later stages never run after an earlier refusal.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap1Stage {
    /// Byte ceiling, UTF-8, strict JSON (duplicate keys, depth, string cap).
    Admission,
    /// The pinned JSON Schema, or the typed shape it is read into.
    Schema,
    /// R0–R8.
    Rules,
}

/// A strict-syntax fault, as a static token. The offending key or position is not carried.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap1SyntaxFault {
    NotUtf8,
    DuplicateKey,
    InvalidEscape,
    NestingTooDeep,
    TooManyKeys,
    StringTooLong,
    Malformed,
}

impl From<&StrictJsonError> for Cap1SyntaxFault {
    fn from(e: &StrictJsonError) -> Self {
        match e {
            StrictJsonError::DuplicateKey { .. } => Self::DuplicateKey,
            StrictJsonError::InvalidUnicodeEscape { .. }
            | StrictJsonError::LoneSurrogate { .. } => Self::InvalidEscape,
            StrictJsonError::NestingTooDeep { .. } => Self::NestingTooDeep,
            StrictJsonError::TooManyKeys { .. } => Self::TooManyKeys,
            StrictJsonError::StringTooLong { .. } => Self::StringTooLong,
            _ => Self::Malformed,
        }
    }
}

/// Why a document was refused, at the first stage and check that refused it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cap1Refusal {
    /// The byte ceiling was crossed; nothing was parsed.
    Oversized { max_bytes: usize },
    /// Not UTF-8 or not strict JSON.
    Syntax(Cap1SyntaxFault),
    /// The pinned schema rejected the instance at `instance_path` under `schema_path`, or the
    /// typed shape rejected an instance the schema accepted (the typed count domain is exact
    /// integers; JSON Schema's `integer` also admits `6.0`). Both are shape.
    Schema {
        instance_path: String,
        schema_path: String,
    },
    /// The first normative rule that failed, in numeric order, with a static grounds string
    /// and the JSON pointer of the stratum, unit or assertion it failed at.
    Rule {
        rule: Cap1NormativeRule,
        grounds: &'static str,
        at: Option<String>,
    },
}

impl Cap1Refusal {
    #[must_use]
    pub fn stage(&self) -> Cap1Stage {
        match self {
            Self::Oversized { .. } | Self::Syntax(_) => Cap1Stage::Admission,
            Self::Schema { .. } => Cap1Stage::Schema,
            Self::Rule { .. } => Cap1Stage::Rules,
        }
    }

    /// The rule that refused, when the rules stage was reached.
    #[must_use]
    pub fn rule(&self) -> Option<Cap1NormativeRule> {
        match self {
            Self::Rule { rule, .. } => Some(*rule),
            _ => None,
        }
    }
}

impl fmt::Display for Cap1Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Oversized { max_bytes } => {
                write!(f, "cap-1 admission: document exceeds {max_bytes} bytes")
            }
            Self::Syntax(fault) => write!(f, "cap-1 admission: strict JSON {fault:?}"),
            Self::Schema {
                instance_path,
                schema_path,
            } => write!(
                f,
                "cap-1 schema: instance {instance_path:?} rejected by {schema_path:?}"
            ),
            Self::Rule { rule, grounds, at } => match at {
                Some(at) => write!(f, "cap-1 {}: {grounds} at {at}", rule.as_str()),
                None => write!(f, "cap-1 {}: {grounds}", rule.as_str()),
            },
        }
    }
}

impl std::error::Error for Cap1Refusal {}

fn schema_validator() -> &'static jsonschema::Validator {
    static VALIDATOR: OnceLock<jsonschema::Validator> = OnceLock::new();
    VALIDATOR.get_or_init(|| {
        let schema: Value =
            serde_json::from_str(CAP1_SCHEMA_JSON).expect("embedded CAP-1 schema is JSON");
        // The pinned schema carries no `$ref`, and the workspace `jsonschema` build has no
        // retrieval feature, so nothing can be fetched during validation.
        jsonschema::options()
            .with_draft(jsonschema::Draft::Draft202012)
            .build(&schema)
            .expect("embedded CAP-1 schema compiles")
    })
}

/// Admit, schema-validate and rule-check one CAP-1 document.
///
/// Returns the typed document the relying-party gate consumes, or the first refusal.
pub fn verify_cap1_document(
    bytes: &[u8],
    limits: &Cap1AdmissionLimits,
) -> Result<Cap1Document, Cap1Refusal> {
    let max_bytes = limits.effective_max_bytes();
    if bytes.len() > max_bytes {
        return Err(Cap1Refusal::Oversized { max_bytes });
    }
    let text =
        std::str::from_utf8(bytes).map_err(|_| Cap1Refusal::Syntax(Cap1SyntaxFault::NotUtf8))?;
    validate_json_strict(text).map_err(|e| Cap1Refusal::Syntax((&e).into()))?;
    let instance: Value =
        serde_json::from_str(text).map_err(|_| Cap1Refusal::Syntax(Cap1SyntaxFault::Malformed))?;

    if let Err(e) = schema_validator().validate(&instance) {
        return Err(Cap1Refusal::Schema {
            instance_path: e.instance_path().to_string(),
            schema_path: e.schema_path().to_string(),
        });
    }
    let doc: Cap1Document = serde_json::from_value(instance).map_err(|_| Cap1Refusal::Schema {
        instance_path: String::new(),
        schema_path: "typed-shape".to_string(),
    })?;

    verify_cap1_rules_with(&doc, &[])?;
    Ok(doc)
}
