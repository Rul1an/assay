//! CAP-1 normative validation for the standalone replay verifier.
//!
//! This module intentionally implements one pinned profile only:
//! `Certisyn-Inc/certisyn-drafts@0980d3201aa2caab3cbad5c6e9bc99b422370b43`
//! `cap-1/src/CAP-1.schema.json`.
//! No generic JSON Schema runtime is linked.

use std::collections::{BTreeSet, HashSet};
use std::fmt;
use std::str::CharIndices;

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
    InvalidEscape,
    NestingTooDeep,
    TooManyKeys,
    StringTooLong,
    Malformed,
}

impl From<&StrictJsonError> for Cap1SyntaxFault {
    fn from(err: &StrictJsonError) -> Self {
        match err {
            StrictJsonError::DuplicateKey => Self::DuplicateKey,
            StrictJsonError::InvalidUnicodeEscape | StrictJsonError::LoneSurrogate => {
                Self::InvalidEscape
            }
            StrictJsonError::NestingTooDeep => Self::NestingTooDeep,
            StrictJsonError::TooManyKeys => Self::TooManyKeys,
            StrictJsonError::StringTooLong => Self::StringTooLong,
            StrictJsonError::ParseError => Self::Malformed,
        }
    }
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
            Self::Syntax(Cap1SyntaxFault::InvalidEscape) => Some("invalid-escape"),
            Self::Syntax(Cap1SyntaxFault::NestingTooDeep) => Some("nesting-too-deep"),
            Self::Syntax(Cap1SyntaxFault::TooManyKeys) => Some("too-many-keys"),
            Self::Syntax(Cap1SyntaxFault::StringTooLong) => Some("string-too-long"),
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

const MAX_NESTING_DEPTH: usize = 64;
const MAX_KEYS_PER_OBJECT: usize = 10_000;
const MAX_STRING_LENGTH: usize = 1_048_576;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StrictJsonError {
    DuplicateKey,
    InvalidUnicodeEscape,
    LoneSurrogate,
    ParseError,
    NestingTooDeep,
    TooManyKeys,
    StringTooLong,
}

struct ObjectKeyTracker {
    stack: Vec<HashSet<String>>,
}

impl ObjectKeyTracker {
    fn new() -> Self {
        Self { stack: Vec::new() }
    }

    fn enter_object(&mut self) {
        self.stack.push(HashSet::new());
    }

    fn push_key(&mut self, key: String) -> Result<(), StrictJsonError> {
        if let Some(keys) = self.stack.last_mut() {
            if keys.len() >= MAX_KEYS_PER_OBJECT {
                return Err(StrictJsonError::TooManyKeys);
            }
            if !keys.insert(key) {
                return Err(StrictJsonError::DuplicateKey);
            }
        }
        Ok(())
    }

    fn exit_object(&mut self) {
        self.stack.pop();
    }
}

struct JsonValidator<'a> {
    chars: std::iter::Peekable<CharIndices<'a>>,
    key_tracker: ObjectKeyTracker,
    depth: usize,
}

impl<'a> JsonValidator<'a> {
    fn new(input: &'a str) -> Self {
        Self {
            chars: input.char_indices().peekable(),
            key_tracker: ObjectKeyTracker::new(),
            depth: 0,
        }
    }

    fn validate(&mut self) -> Result<(), StrictJsonError> {
        self.skip_whitespace();
        self.validate_value()
    }

    fn validate_value(&mut self) -> Result<(), StrictJsonError> {
        self.skip_whitespace();
        match self.peek_char() {
            Some('{') => self.validate_object(),
            Some('[') => self.validate_array(),
            Some('"') => self.validate_string().map(|_| ()),
            Some(c) if c == '-' || c.is_ascii_digit() => self.validate_number(),
            Some('t') | Some('f') => self.validate_bool(),
            Some('n') => self.validate_null(),
            Some(_) => Err(StrictJsonError::ParseError),
            None => Ok(()),
        }
    }

    fn validate_object(&mut self) -> Result<(), StrictJsonError> {
        self.expect_char('{')?;
        self.skip_whitespace();

        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            return Err(StrictJsonError::NestingTooDeep);
        }
        self.key_tracker.enter_object();
        if self.peek_char() == Some('}') {
            self.next_char();
            self.key_tracker.exit_object();
            self.depth -= 1;
            return Ok(());
        }

        loop {
            self.skip_whitespace();
            let key = self.validate_string()?;
            self.key_tracker.push_key(key)?;

            self.skip_whitespace();
            self.expect_char(':')?;
            self.validate_value()?;
            self.skip_whitespace();

            match self.peek_char() {
                Some(',') => {
                    self.next_char();
                }
                Some('}') => {
                    self.next_char();
                    self.key_tracker.exit_object();
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(StrictJsonError::ParseError),
            }
        }
    }

    fn validate_array(&mut self) -> Result<(), StrictJsonError> {
        self.expect_char('[')?;
        self.skip_whitespace();

        self.depth += 1;
        if self.depth > MAX_NESTING_DEPTH {
            return Err(StrictJsonError::NestingTooDeep);
        }

        if self.peek_char() == Some(']') {
            self.next_char();
            self.depth -= 1;
            return Ok(());
        }

        loop {
            self.validate_value()?;
            self.skip_whitespace();

            match self.peek_char() {
                Some(',') => {
                    self.next_char();
                    self.skip_whitespace();
                }
                Some(']') => {
                    self.next_char();
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(StrictJsonError::ParseError),
            }
        }
    }

    fn validate_string(&mut self) -> Result<String, StrictJsonError> {
        parse_json_string(&mut self.chars)
    }

    fn validate_number(&mut self) -> Result<(), StrictJsonError> {
        if self.peek_char() == Some('-') {
            self.next_char();
        }

        match self.peek_char() {
            Some('0') => {
                self.next_char();
            }
            Some(c) if c.is_ascii_digit() => {
                while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                    self.next_char();
                }
            }
            _ => return Err(StrictJsonError::ParseError),
        }

        if self.peek_char() == Some('.') {
            self.next_char();
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(StrictJsonError::ParseError);
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char();
            }
        }

        if matches!(self.peek_char(), Some('e') | Some('E')) {
            self.next_char();
            if matches!(self.peek_char(), Some('+') | Some('-')) {
                self.next_char();
            }
            if !self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                return Err(StrictJsonError::ParseError);
            }
            while self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                self.next_char();
            }
        }

        Ok(())
    }

    fn validate_bool(&mut self) -> Result<(), StrictJsonError> {
        if self.consume_keyword("true") || self.consume_keyword("false") {
            Ok(())
        } else {
            Err(StrictJsonError::ParseError)
        }
    }

    fn validate_null(&mut self) -> Result<(), StrictJsonError> {
        if self.consume_keyword("null") {
            Ok(())
        } else {
            Err(StrictJsonError::ParseError)
        }
    }

    fn consume_keyword(&mut self, keyword: &str) -> bool {
        let mut temp_chars = self.chars.clone();
        for expected in keyword.chars() {
            match temp_chars.next() {
                Some((_, c)) if c == expected => {}
                _ => return false,
            }
        }
        self.chars = temp_chars;
        true
    }

    fn skip_whitespace(&mut self) {
        while self.peek_char().is_some_and(|c| c.is_whitespace()) {
            self.next_char();
        }
    }

    fn peek_char(&mut self) -> Option<char> {
        self.chars.peek().map(|(_, c)| *c)
    }

    fn next_char(&mut self) -> Option<(usize, char)> {
        self.chars.next()
    }

    fn expect_char(&mut self, expected: char) -> Result<(), StrictJsonError> {
        match self.next_char() {
            Some((_, c)) if c == expected => Ok(()),
            _ => Err(StrictJsonError::ParseError),
        }
    }
}

fn parse_json_string<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
) -> Result<String, StrictJsonError> {
    match chars.next() {
        Some((_, '"')) => {}
        _ => return Err(StrictJsonError::ParseError),
    }

    let mut result = String::new();
    let mut prev_high_surrogate: Option<u32> = None;
    let mut char_count = 0usize;

    fn push_with_limit(
        result: &mut String,
        c: char,
        char_count: &mut usize,
    ) -> Result<(), StrictJsonError> {
        *char_count += 1;
        if *char_count > MAX_STRING_LENGTH {
            return Err(StrictJsonError::StringTooLong);
        }
        result.push(c);
        Ok(())
    }

    loop {
        match chars.next() {
            Some((_, '"')) => {
                if prev_high_surrogate.is_some() {
                    return Err(StrictJsonError::LoneSurrogate);
                }
                return Ok(result);
            }
            Some((pos, '\\')) => match chars.next() {
                Some((_, 'u')) => {
                    let codepoint = parse_unicode_escape(chars, pos)?;
                    if (0xD800..=0xDBFF).contains(&codepoint) {
                        if prev_high_surrogate.is_some() {
                            return Err(StrictJsonError::LoneSurrogate);
                        }
                        prev_high_surrogate = Some(codepoint);
                    } else if (0xDC00..=0xDFFF).contains(&codepoint) {
                        if let Some(high) = prev_high_surrogate {
                            let combined = 0x10000 + ((high - 0xD800) << 10) + (codepoint - 0xDC00);
                            if let Some(c) = char::from_u32(combined) {
                                push_with_limit(&mut result, c, &mut char_count)?;
                            } else {
                                return Err(StrictJsonError::InvalidUnicodeEscape);
                            }
                            prev_high_surrogate = None;
                        } else {
                            return Err(StrictJsonError::LoneSurrogate);
                        }
                    } else {
                        if prev_high_surrogate.is_some() {
                            return Err(StrictJsonError::LoneSurrogate);
                        }
                        if let Some(c) = char::from_u32(codepoint) {
                            push_with_limit(&mut result, c, &mut char_count)?;
                        } else {
                            return Err(StrictJsonError::InvalidUnicodeEscape);
                        }
                    }
                }
                Some((_, c)) => {
                    if prev_high_surrogate.is_some() {
                        return Err(StrictJsonError::LoneSurrogate);
                    }
                    let decoded = match c {
                        'n' => '\n',
                        'r' => '\r',
                        't' => '\t',
                        '\\' => '\\',
                        '/' => '/',
                        '"' => '"',
                        'b' => '\x08',
                        'f' => '\x0C',
                        _ => return Err(StrictJsonError::ParseError),
                    };
                    push_with_limit(&mut result, decoded, &mut char_count)?;
                }
                None => return Err(StrictJsonError::ParseError),
            },
            Some((_, c)) => {
                if prev_high_surrogate.is_some() {
                    return Err(StrictJsonError::LoneSurrogate);
                }
                push_with_limit(&mut result, c, &mut char_count)?;
            }
            None => return Err(StrictJsonError::ParseError),
        }
    }
}

fn parse_unicode_escape<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
    _start_pos: usize,
) -> Result<u32, StrictJsonError> {
    let mut hex = String::with_capacity(4);
    for _ in 0..4 {
        match chars.next() {
            Some((_, c)) if c.is_ascii_hexdigit() => hex.push(c),
            _ => return Err(StrictJsonError::InvalidUnicodeEscape),
        }
    }
    u32::from_str_radix(&hex, 16).map_err(|_| StrictJsonError::InvalidUnicodeEscape)
}

fn validate_json_strict(raw: &str) -> Result<(), StrictJsonError> {
    let mut validator = JsonValidator::new(raw);
    validator.validate()
}

fn parse_json_value_strict(raw: &str) -> Result<Value, Cap1Refusal> {
    validate_json_strict(raw).map_err(|err| Cap1Refusal::Syntax((&err).into()))?;
    serde_json::from_str(raw).map_err(|_| Cap1Refusal::Syntax(Cap1SyntaxFault::Malformed))
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
