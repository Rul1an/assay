//! CAP-1 normative validation for the standalone replay verifier.
//!
//! This module intentionally implements one pinned profile only:
//! `Certisyn-Inc/certisyn-drafts@0980d3201aa2caab3cbad5c6e9bc99b422370b43`
//! `cap-1/src/CAP-1.schema.json`.
//! No generic JSON Schema runtime is linked.

use std::collections::BTreeSet;
use std::fmt;

use serde::de::{self, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Embedded bytes for the pinned CAP-1 schema.
pub const CAP1_SCHEMA_JSON: &str =
    include_str!("../../assay-evidence/schemas/cap-1/CAP-1.schema.json");

/// SHA-256 digest of [`CAP1_SCHEMA_JSON`].
pub const CAP1_SCHEMA_SHA256: &str =
    "4453f216089543780bfecc4295cc4a61462fdc585b88d1e35b7d1aba79716b4a";

/// Where the pinned schema bytes come from.
pub const CAP1_SCHEMA_SOURCE: &str =
    "Certisyn-Inc/certisyn-drafts@0980d3201aa2caab3cbad5c6e9bc99b422370b43 cap-1/src/CAP-1.schema.json";

/// Admission limits charged before semantic validation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Cap1AdmissionLimits {
    pub max_bytes: usize,
}

impl Cap1AdmissionLimits {
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

/// Which stage refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap1Stage {
    Admission,
    Schema,
    Rules,
}

/// Strict admission faults.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cap1SyntaxFault {
    NotUtf8,
    DuplicateKey,
    Malformed,
}

/// Why a document was refused at the first failing stage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Cap1Refusal {
    Oversized {
        max_bytes: usize,
    },
    Syntax(Cap1SyntaxFault),
    Schema {
        instance_path: String,
        schema_ground: &'static str,
    },
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

    #[must_use]
    pub fn rule(&self) -> Option<Cap1NormativeRule> {
        match self {
            Self::Rule { rule, .. } => Some(*rule),
            _ => None,
        }
    }

    #[must_use]
    pub fn schema_ground(&self) -> Option<&'static str> {
        match self {
            Self::Schema { schema_ground, .. } => Some(*schema_ground),
            _ => None,
        }
    }

    #[must_use]
    pub fn instance_path(&self) -> Option<&str> {
        match self {
            Self::Schema { instance_path, .. } => Some(instance_path.as_str()),
            _ => None,
        }
    }

    #[must_use]
    pub fn admission_ground(&self) -> Option<&'static str> {
        match self {
            Self::Oversized { .. } => Some("oversized"),
            Self::Syntax(Cap1SyntaxFault::NotUtf8) => Some("not-utf8"),
            Self::Syntax(Cap1SyntaxFault::DuplicateKey) => Some("duplicate-key"),
            Self::Syntax(Cap1SyntaxFault::Malformed) => Some("malformed-json"),
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
                schema_ground,
            } => write!(
                f,
                "cap-1 schema: instance {instance_path:?} rejected by {schema_ground}"
            ),
            Self::Rule { rule, grounds, at } => match at {
                Some(at) => write!(f, "cap-1 {}: {grounds} at {at}", rule.as_str()),
                None => write!(f, "cap-1 {}: {grounds}", rule.as_str()),
            },
        }
    }
}

impl std::error::Error for Cap1Refusal {}

const DUPLICATE_KEY_MARKER: &str = "cap-1 duplicate object key: ";

struct StrictValue(Value);

impl<'de> Deserialize<'de> for StrictValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(StrictVisitor).map(Self)
    }
}

struct StrictVisitor;

impl<'de> Visitor<'de> for StrictVisitor {
    type Value = Value;

    fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("any valid JSON value with unique object keys")
    }

    fn visit_bool<E: de::Error>(self, value: bool) -> Result<Self::Value, E> {
        Ok(Value::Bool(value))
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<Self::Value, E> {
        Ok(Value::from(value))
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<Self::Value, E> {
        Ok(Value::from(value))
    }

    fn visit_f64<E: de::Error>(self, value: f64) -> Result<Self::Value, E> {
        Ok(Value::from(value))
    }

    fn visit_str<E: de::Error>(self, value: &str) -> Result<Self::Value, E> {
        Ok(Value::String(value.to_owned()))
    }

    fn visit_string<E: de::Error>(self, value: String) -> Result<Self::Value, E> {
        Ok(Value::String(value))
    }

    fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
        Ok(Value::Null)
    }

    fn visit_some<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_any(StrictVisitor)
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
        let mut out = Vec::new();
        while let Some(StrictValue(value)) = seq.next_element()? {
            out.push(value);
        }
        Ok(Value::Array(out))
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
        let mut out = serde_json::Map::new();
        while let Some(key) = map.next_key::<String>()? {
            let StrictValue(value) = map.next_value()?;
            if out.insert(key.clone(), value).is_some() {
                return Err(de::Error::custom(format!("{DUPLICATE_KEY_MARKER}{key}")));
            }
        }
        Ok(Value::Object(out))
    }
}

fn parse_json_value_strict(raw: &str) -> Result<Value, Cap1Refusal> {
    serde_json::from_str::<StrictValue>(raw)
        .map(|strict| strict.0)
        .map_err(classify_parse_error)
}

fn classify_parse_error(err: serde_json::Error) -> Cap1Refusal {
    let msg = err.to_string();
    let fault = if msg.starts_with(DUPLICATE_KEY_MARKER) {
        Cap1SyntaxFault::DuplicateKey
    } else {
        Cap1SyntaxFault::Malformed
    };
    Cap1Refusal::Syntax(fault)
}

fn schema_refusal(path: impl Into<String>, ground: &'static str) -> Cap1Refusal {
    Cap1Refusal::Schema {
        instance_path: path.into(),
        schema_ground: ground,
    }
}

/// The typed CAP-1 document consumed by R0-R8.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Document {
    pub profile: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub producer: Option<Cap1Producer>,
    pub subject: Cap1Subject,
    pub strata: Vec<Cap1Stratum>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub absence_assertions: Option<Vec<Cap1AbsenceAssertion>>,
    pub integrity: Cap1Integrity,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub as_of: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Producer {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub policy: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Subject {
    pub kind: String,
    #[serde(rename = "ref")]
    pub reference: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub digest: Option<Cap1Digest>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Digest {
    pub algorithm: String,
    pub value: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Stratum {
    pub id: String,
    pub population: String,
    pub basis: Cap1Basis,
    pub eligible: u64,
    pub examined: u64,
    pub unexamined: Vec<Cap1Unexamined>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub supports: Option<Vec<String>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Basis {
    pub kind: Cap1BasisKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue_digest: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub catalogue_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub enumeration_method: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap1BasisKind {
    Catalogue,
    Enumeration,
    Declared,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Unexamined {
    pub unit: String,
    pub disposition: Cap1Disposition,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub detail: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub withheld_digest: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Cap1Disposition {
    NotApplicable,
    DisabledByPolicy,
    UnsupportedInput,
    ResourceExhausted,
    Failed,
    Unavailable,
    OutOfScope,
    Withheld,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1AbsenceAssertion {
    pub assertion: String,
    pub stratum: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub qualifier: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Cap1Integrity {
    pub complete: bool,
    pub statement: String,
    #[serde(default)]
    pub uncapped_verdict: Option<String>,
    #[serde(default)]
    pub capped_to: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub unaccounted: Option<Vec<String>>,
}

/// R0-R8 ids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Cap1NormativeRule {
    R0Shape,
    R1NoSilentRemainder,
    R2ClosedDisposition,
    R3WithholdingDigestBound,
    R4DenominatorBasis,
    R5CountsWellFormed,
    R6AbsenceIsScoped,
    R7IncompleteNotClean,
    R8SupportsBoundsCitation,
}

impl Cap1NormativeRule {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::R0Shape => "R0-shape",
            Self::R1NoSilentRemainder => "R1-no-silent-remainder",
            Self::R2ClosedDisposition => "R2-closed-disposition",
            Self::R3WithholdingDigestBound => "R3-withholding-digest-bound",
            Self::R4DenominatorBasis => "R4-denominator-basis",
            Self::R5CountsWellFormed => "R5-counts-well-formed",
            Self::R6AbsenceIsScoped => "R6-absence-is-scoped",
            Self::R7IncompleteNotClean => "R7-incomplete-not-clean",
            Self::R8SupportsBoundsCitation => "R8-supports-bounds-citation",
        }
    }
}

fn is_hex_digest(s: &str) -> bool {
    (32..=128).contains(&s.len())
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn is_stratum_id(s: &str) -> bool {
    let mut bytes = s.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    let first_ok = first.is_ascii_digit() || first.is_ascii_lowercase();
    first_ok
        && bytes.all(|b| {
            b.is_ascii_digit() || b.is_ascii_lowercase() || matches!(b, b'.' | b'_' | b'-')
        })
}

fn validate_schema_shape(instance: &Value) -> Result<(), Cap1Refusal> {
    let root = instance
        .as_object()
        .ok_or_else(|| schema_refusal("", "typed-shape"))?;

    if root.get("profile").and_then(Value::as_str) != Some("cap/1") {
        return Err(schema_refusal("/profile", "const-profile-cap-1"));
    }

    let strata = root
        .get("strata")
        .and_then(Value::as_array)
        .ok_or_else(|| schema_refusal("/strata", "typed-shape"))?;
    if strata.is_empty() {
        return Err(schema_refusal("/strata", "min-items"));
    }

    for (i, stratum) in strata.iter().enumerate() {
        let path = format!("/strata/{i}");
        let stratum_obj = stratum
            .as_object()
            .ok_or_else(|| schema_refusal(path.clone(), "typed-shape"))?;

        if let Some(id) = stratum_obj.get("id").and_then(Value::as_str) {
            if !is_stratum_id(id) {
                return Err(schema_refusal(format!("{path}/id"), "stratum-id-pattern"));
            }
        }

        if let Some(basis) = stratum_obj.get("basis").and_then(Value::as_object) {
            if let Some(kind) = basis.get("kind").and_then(Value::as_str) {
                if !matches!(kind, "catalogue" | "enumeration" | "declared") {
                    return Err(schema_refusal(format!("{path}/basis/kind"), "basis-kind"));
                }
            }
            if let Some(digest) = basis.get("catalogue_digest").and_then(Value::as_str) {
                if !is_hex_digest(digest) {
                    return Err(schema_refusal(
                        format!("{path}/basis/catalogue_digest"),
                        "hex-digest-pattern",
                    ));
                }
            }
        }

        for (field, value) in [
            ("eligible", stratum_obj.get("eligible")),
            ("examined", stratum_obj.get("examined")),
        ] {
            if let Some(value) = value {
                if value.as_u64().is_some() {
                    continue;
                }
                if value.as_i64().is_some_and(|n| n < 0) {
                    return Err(schema_refusal(format!("{path}/{field}"), "minimum"));
                }
                return Err(schema_refusal(format!("{path}/{field}"), "typed-shape"));
            }
        }

        if let Some(unexamined) = stratum_obj.get("unexamined").and_then(Value::as_array) {
            for (j, unit) in unexamined.iter().enumerate() {
                let unit_path = format!("{path}/unexamined/{j}");
                let unit_obj = unit
                    .as_object()
                    .ok_or_else(|| schema_refusal(unit_path.clone(), "typed-shape"))?;
                if let Some(disposition) = unit_obj.get("disposition").and_then(Value::as_str) {
                    if !matches!(
                        disposition,
                        "not_applicable"
                            | "disabled_by_policy"
                            | "unsupported_input"
                            | "resource_exhausted"
                            | "failed"
                            | "unavailable"
                            | "out_of_scope"
                            | "withheld"
                    ) {
                        return Err(schema_refusal(
                            format!("{unit_path}/disposition"),
                            "closed-disposition",
                        ));
                    }
                }
                if let Some(withheld_digest) =
                    unit_obj.get("withheld_digest").and_then(Value::as_str)
                {
                    if !is_hex_digest(withheld_digest) {
                        return Err(schema_refusal(
                            format!("{unit_path}/withheld_digest"),
                            "hex-digest-pattern",
                        ));
                    }
                }
            }
        }
    }

    if let Some(digest_value) = root
        .get("subject")
        .and_then(Value::as_object)
        .and_then(|subject| subject.get("digest"))
        .and_then(Value::as_object)
        .and_then(|digest| digest.get("value"))
        .and_then(Value::as_str)
    {
        if !is_hex_digest(digest_value) {
            return Err(schema_refusal(
                "/subject/digest/value",
                "hex-digest-pattern",
            ));
        }
    }

    Ok(())
}

/// Admission -> pinned schema shape -> R0-R8.
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
    let value: Value = parse_json_value_strict(text)?;

    validate_schema_shape(&value)?;
    let doc: Cap1Document =
        serde_json::from_value(value).map_err(|_| schema_refusal("", "typed-shape"))?;
    verify_cap1_rules_with(&doc, &[])?;
    Ok(doc)
}

fn is_hard_stop(d: Cap1Disposition) -> bool {
    matches!(
        d,
        Cap1Disposition::Failed | Cap1Disposition::ResourceExhausted | Cap1Disposition::Unavailable
    )
}

fn refuse(rule: Cap1NormativeRule, grounds: &'static str, at: Option<String>) -> Cap1Refusal {
    Cap1Refusal::Rule { rule, grounds, at }
}

fn stratum_at(i: usize) -> Option<String> {
    Some(format!("/strata/{i}"))
}

fn unit_at(i: usize, j: usize) -> Option<String> {
    Some(format!("/strata/{i}/unexamined/{j}"))
}

pub fn verify_cap1_rules(doc: &Cap1Document) -> Result<(), Cap1Refusal> {
    verify_cap1_rules_with(doc, &[])
}

pub fn verify_cap1_rules_with(
    doc: &Cap1Document,
    disabled: &[Cap1NormativeRule],
) -> Result<(), Cap1Refusal> {
    use Cap1NormativeRule as R;
    let on = |r: R| !disabled.contains(&r);

    if on(R::R0Shape) {
        if doc.profile != "cap/1" {
            return Err(refuse(R::R0Shape, "profile is not cap/1", None));
        }
        if doc.strata.is_empty() {
            return Err(refuse(R::R0Shape, "no strata", None));
        }
        let mut ids = BTreeSet::new();
        for (i, s) in doc.strata.iter().enumerate() {
            if s.id.is_empty() {
                return Err(refuse(R::R0Shape, "stratum id absent", stratum_at(i)));
            }
            if !ids.insert(s.id.as_str()) {
                return Err(refuse(R::R0Shape, "stratum id duplicated", stratum_at(i)));
            }
        }
    }

    if on(R::R1NoSilentRemainder) {
        for (i, s) in doc.strata.iter().enumerate() {
            let accounted = u64::try_from(s.unexamined.len()).ok();
            let reconciled = accounted
                .and_then(|n| s.examined.checked_add(n))
                .is_some_and(|sum| sum == s.eligible);
            if !reconciled {
                return Err(refuse(
                    R::R1NoSilentRemainder,
                    "eligible does not equal examined plus accounted unexamined",
                    stratum_at(i),
                ));
            }
        }
    }

    if on(R::R2ClosedDisposition) {
        for (i, s) in doc.strata.iter().enumerate() {
            for (j, u) in s.unexamined.iter().enumerate() {
                if u.unit.is_empty() {
                    return Err(refuse(
                        R::R2ClosedDisposition,
                        "unexamined entry names no unit",
                        unit_at(i, j),
                    ));
                }
            }
        }
    }

    if on(R::R3WithholdingDigestBound) {
        for (i, s) in doc.strata.iter().enumerate() {
            for (j, u) in s.unexamined.iter().enumerate() {
                if u.disposition == Cap1Disposition::Withheld
                    && !u.withheld_digest.as_deref().is_some_and(is_hex_digest)
                {
                    return Err(refuse(
                        R::R3WithholdingDigestBound,
                        "withheld unit carries no digest of the withheld value",
                        unit_at(i, j),
                    ));
                }
            }
        }
    }

    if on(R::R4DenominatorBasis) {
        for (i, s) in doc.strata.iter().enumerate() {
            let ok = match s.basis.kind {
                Cap1BasisKind::Catalogue => s
                    .basis
                    .catalogue_digest
                    .as_deref()
                    .is_some_and(is_hex_digest),
                Cap1BasisKind::Enumeration => s
                    .basis
                    .enumeration_method
                    .as_deref()
                    .is_some_and(|m| !m.is_empty()),
                Cap1BasisKind::Declared => true,
            };
            if !ok {
                return Err(refuse(
                    R::R4DenominatorBasis,
                    "basis lacks the digest or method its kind requires",
                    stratum_at(i),
                ));
            }
        }
    }

    if on(R::R5CountsWellFormed) {
        for (i, s) in doc.strata.iter().enumerate() {
            if s.examined > s.eligible {
                return Err(refuse(
                    R::R5CountsWellFormed,
                    "examined exceeds eligible",
                    stratum_at(i),
                ));
            }
        }
    }

    let ids: BTreeSet<&str> = doc.strata.iter().map(|s| s.id.as_str()).collect();
    let assertions = doc.absence_assertions.as_deref().unwrap_or(&[]);

    if on(R::R6AbsenceIsScoped) {
        for (k, a) in assertions.iter().enumerate() {
            if a.stratum.is_empty() || !ids.contains(a.stratum.as_str()) {
                return Err(refuse(
                    R::R6AbsenceIsScoped,
                    "absence assertion names no stratum, or an unknown one",
                    Some(format!("/absence_assertions/{k}")),
                ));
            }
        }
    }

    if on(R::R7IncompleteNotClean) {
        let any_hard_stop = doc
            .strata
            .iter()
            .flat_map(|s| s.unexamined.iter())
            .any(|u| is_hard_stop(u.disposition));
        if any_hard_stop && doc.integrity.complete {
            return Err(refuse(
                R::R7IncompleteNotClean,
                "integrity.complete is true while units failed, were exhausted or unavailable",
                Some("/integrity/complete".to_string()),
            ));
        }
        let capped = doc
            .integrity
            .capped_to
            .as_deref()
            .is_some_and(|c| !c.is_empty());
        if !doc.integrity.complete && !capped {
            return Err(refuse(
                R::R7IncompleteNotClean,
                "integrity.complete is false and no capped_to verdict is stated",
                Some("/integrity/capped_to".to_string()),
            ));
        }
    }

    if on(R::R8SupportsBoundsCitation) {
        let cited: BTreeSet<&str> = assertions.iter().map(|a| a.stratum.as_str()).collect();
        for (i, s) in doc.strata.iter().enumerate() {
            let states_supports = s.supports.as_deref().is_some_and(|v| !v.is_empty());
            if cited.contains(s.id.as_str()) && !states_supports {
                return Err(refuse(
                    R::R8SupportsBoundsCitation,
                    "stratum is cited by an absence assertion but states no supported claim classes",
                    stratum_at(i),
                ));
            }
        }
    }

    Ok(())
}
