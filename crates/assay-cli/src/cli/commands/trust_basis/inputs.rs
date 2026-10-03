//! Input-provenance sidecar for Trust Basis reports: the closed
//! `assay.trust-basis.inputs.v0` contract, its bounded validator, and the
//! `assay.trust-basis.inputs-check.v0` result.
//!
//! The sidecar is an unsigned statement of which report bytes were produced from
//! which input bytes, packs and limits. A pair that validates is bound to itself:
//! it says nothing about who wrote it, whether it is current, or whether the
//! report's claims are true.

use assay_evidence::{
    duplicate_trust_basis_claim_ids, to_canonical_json_bytes, TrustBasis, VerifyLimits,
};
use serde::de::{self, DeserializeSeed, MapAccess, SeqAccess, Visitor};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::cell::Cell;
use std::collections::HashSet;
use std::fmt;
use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

pub(super) const REPORT_FILE: &str = "trust-basis.json";
pub(super) const INPUTS_FILE: &str = "trust-basis.inputs.json";
pub(super) const INPUTS_SCHEMA: &str = "assay.trust-basis.inputs.v0";
const CHECK_SCHEMA: &str = "assay.trust-basis.inputs-check.v0";
pub(super) const PACK_DIGEST_DOMAIN: &str = "assay.parsed-pack-definition.jcs.sha256";

/// Largest integer every common JSON reader represents exactly (2^53 - 1).
pub(super) const MAX_WIRE_INTEGER: u64 = (1 << 53) - 1;
const MAX_SIDECAR_BYTES: u64 = 64 * 1024;
const MAX_REPORT_BYTES: u64 = 1024 * 1024;

/// The two serialized members of an artifact directory and the byte ceiling each is
/// admitted under. The reader's bounded read and the writer's pre-publication check
/// both answer [`Member::admits`], so a writer cannot publish a member the reader
/// refuses for its size.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Member {
    Report,
    Sidecar,
}

impl Member {
    fn ceiling(self) -> u64 {
        match self {
            Self::Report => MAX_REPORT_BYTES,
            Self::Sidecar => MAX_SIDECAR_BYTES,
        }
    }

    fn admits(self, len: u64) -> bool {
        len <= self.ceiling()
    }
}
/// Nesting ceiling for the sidecar and report documents; unrelated to the bundle
/// verifier's `max_json_depth`.
const MAX_ARTIFACT_JSON_DEPTH: usize = 16;
const MAX_PACKS: usize = 64;
const MAX_METADATA_BYTES: usize = 256;

const NOT_ESTABLISHED: [&str; 5] = [
    "claim_set_completeness",
    "environment_completeness",
    "freshness",
    "generation_authenticity",
    "pack_execution",
];

// ---------------------------------------------------------------- sidecar model

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct InputsV0 {
    pub schema: String,
    pub report: ByteIdentityV0,
    pub bundle: ByteIdentityV0,
    pub pack_digest_domain: String,
    pub packs: Vec<PackV0>,
    pub limits: LimitsV0,
    pub lint: LintV0,
    pub reported_assay_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ByteIdentityV0 {
    pub sha256: String,
    pub bytes: u64,
}

impl ByteIdentityV0 {
    pub(super) fn of(bytes: &[u8]) -> Self {
        Self {
            sha256: sha256_hex(bytes),
            bytes: bytes.len() as u64,
        }
    }
}

/// The one binding comparison: recorded digest and length against observed ones.
/// The reader and the writer's self-check both answer it here.
fn identity_matches(recorded: &ByteIdentityV0, sha256: &str, bytes: u64) -> bool {
    recorded.bytes == bytes && recorded.sha256 == sha256
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct PackV0 {
    pub name: String,
    pub version: String,
    pub source_kind: String,
    pub digest: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LimitsV0 {
    pub max_bundle_bytes: u64,
    pub max_decode_bytes: u64,
    pub max_manifest_bytes: u64,
    pub max_events_bytes: u64,
    pub max_events: u64,
    pub max_line_bytes: u64,
    pub max_path_len: u64,
    pub max_json_depth: u64,
}

impl LimitsV0 {
    /// Record the limits value generation ran with, field for field.
    pub(super) fn from_limits(limits: &VerifyLimits) -> Self {
        Self {
            max_bundle_bytes: limits.max_bundle_bytes,
            max_decode_bytes: limits.max_decode_bytes,
            max_manifest_bytes: limits.max_manifest_bytes,
            max_events_bytes: limits.max_events_bytes,
            max_events: limits.max_events as u64,
            max_line_bytes: limits.max_line_bytes as u64,
            max_path_len: limits.max_path_len as u64,
            max_json_depth: limits.max_json_depth as u64,
        }
    }

    fn values(&self) -> [u64; 8] {
        [
            self.max_bundle_bytes,
            self.max_decode_bytes,
            self.max_manifest_bytes,
            self.max_events_bytes,
            self.max_events,
            self.max_line_bytes,
            self.max_path_len,
            self.max_json_depth,
        ]
    }
}

/// Lint state is a plain struct, not a boolean-tagged enum: `enabled` is always
/// present, `max_results` is present exactly when `enabled` is true. Serde reads a
/// missing and a `null` `max_results` the same way; the canonical round-trip in
/// [`validate_inputs`] is what refuses the `null` spelling.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct LintV0 {
    pub enabled: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_results: Option<u64>,
}

/// The one renderer for documents this module writes: pretty JSON, two-space
/// indentation, one trailing newline. It matches the existing Trust Basis report
/// renderer, which stays the authority for `trust-basis.json`.
pub(super) fn render_pretty<T: Serialize>(value: &T) -> io::Result<Vec<u8>> {
    let mut output = Vec::new();
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut serializer = serde_json::Serializer::with_formatter(&mut output, formatter);
    value.serialize(&mut serializer).map_err(io::Error::other)?;
    output.push(b'\n');
    Ok(output)
}

// ---------------------------------------------------------------- refusal reasons

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Refusal {
    MalformedJson,
    DuplicateKey,
    DepthExceeded,
    NumberInvalid,
    UnsupportedSchema,
    ShapeInvalid,
    InconsistentLintState,
    ValueOutOfRange,
    DuplicateClaimId,
    NotCanonical,
}

impl Refusal {
    pub(super) fn code(self) -> &'static str {
        match self {
            Self::MalformedJson => "malformed_json",
            Self::DuplicateKey => "duplicate_key",
            Self::DepthExceeded => "depth_exceeded",
            Self::NumberInvalid => "number_invalid",
            Self::UnsupportedSchema => "unsupported_schema",
            Self::ShapeInvalid => "shape_invalid",
            Self::InconsistentLintState => "inconsistent_lint_state",
            Self::ValueOutOfRange => "value_out_of_range",
            Self::DuplicateClaimId => "duplicate_claim_id",
            Self::NotCanonical => "not_canonical",
        }
    }
}

// ---------------------------------------------------------------- structural scan

/// Walk a JSON document once before any typed parse and refuse what typed parsing
/// would silently accept or lose: a repeated key in any object, nesting beyond the
/// artifact ceiling, and any number that is not a wire integer. Ordinary `Value`
/// parsing keeps the last duplicate, so it cannot be the detector.
fn scan_structure(bytes: &[u8]) -> Result<(), Refusal> {
    let fault = Cell::new(None);
    let mut deserializer = serde_json::Deserializer::from_slice(bytes);
    let scanned = Scan {
        depth: 0,
        fault: &fault,
    }
    .deserialize(&mut deserializer)
    .and_then(|()| deserializer.end());
    match (scanned, fault.get()) {
        (Ok(()), _) => Ok(()),
        (Err(_), Some(refusal)) => Err(refusal),
        (Err(_), None) => Err(Refusal::MalformedJson),
    }
}

#[derive(Clone, Copy)]
struct Scan<'a> {
    depth: usize,
    fault: &'a Cell<Option<Refusal>>,
}

impl Scan<'_> {
    fn refuse<E: de::Error>(&self, refusal: Refusal) -> E {
        self.fault.set(Some(refusal));
        E::custom("refused")
    }

    fn nested(&self) -> Self {
        Self {
            depth: self.depth + 1,
            fault: self.fault,
        }
    }
}

impl<'de> DeserializeSeed<'de> for Scan<'_> {
    type Value = ();

    fn deserialize<D: de::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_any(self)
    }
}

impl<'de> Visitor<'de> for Scan<'_> {
    type Value = ();

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("a JSON value")
    }

    fn visit_bool<E: de::Error>(self, _: bool) -> Result<(), E> {
        Ok(())
    }

    fn visit_unit<E: de::Error>(self) -> Result<(), E> {
        Ok(())
    }

    fn visit_str<E: de::Error>(self, _: &str) -> Result<(), E> {
        Ok(())
    }

    fn visit_u64<E: de::Error>(self, value: u64) -> Result<(), E> {
        if value > MAX_WIRE_INTEGER {
            return Err(self.refuse(Refusal::NumberInvalid));
        }
        Ok(())
    }

    fn visit_i64<E: de::Error>(self, value: i64) -> Result<(), E> {
        if value < 0 {
            return Err(self.refuse(Refusal::NumberInvalid));
        }
        self.visit_u64(value as u64)
    }

    fn visit_f64<E: de::Error>(self, _: f64) -> Result<(), E> {
        Err(self.refuse(Refusal::NumberInvalid))
    }

    fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<(), A::Error> {
        let inner = self.nested();
        if inner.depth > MAX_ARTIFACT_JSON_DEPTH {
            return Err(self.refuse(Refusal::DepthExceeded));
        }
        while seq.next_element_seed(inner)?.is_some() {}
        Ok(())
    }

    fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
        let inner = self.nested();
        if inner.depth > MAX_ARTIFACT_JSON_DEPTH {
            return Err(self.refuse(Refusal::DepthExceeded));
        }
        let mut keys = HashSet::new();
        while let Some(key) = map.next_key::<String>()? {
            if !keys.insert(key) {
                return Err(self.refuse(Refusal::DuplicateKey));
            }
            map.next_value_seed(inner)?;
        }
        Ok(())
    }
}

// ---------------------------------------------------------------- validators

fn is_lower_hex_sha256(value: &str) -> bool {
    value.len() == 64
        && value
            .bytes()
            .all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

fn is_bounded_metadata(value: &str) -> bool {
    !value.is_empty() && value.len() <= MAX_METADATA_BYTES
}

/// Validate sidecar bytes against the closed v0 contract and return the parsed model.
pub(super) fn validate_inputs(bytes: &[u8]) -> Result<InputsV0, Refusal> {
    scan_structure(bytes)?;
    let inputs: InputsV0 = serde_json::from_slice(bytes).map_err(|_| Refusal::ShapeInvalid)?;
    if inputs.schema != INPUTS_SCHEMA {
        return Err(Refusal::UnsupportedSchema);
    }
    if inputs.pack_digest_domain != PACK_DIGEST_DOMAIN
        || !is_lower_hex_sha256(&inputs.report.sha256)
        || !is_lower_hex_sha256(&inputs.bundle.sha256)
        || !is_bounded_metadata(&inputs.reported_assay_version)
        || inputs.packs.len() > MAX_PACKS
    {
        return Err(Refusal::ShapeInvalid);
    }
    for pack in &inputs.packs {
        let digest_ok = pack
            .digest
            .strip_prefix("sha256:")
            .is_some_and(is_lower_hex_sha256);
        if !digest_ok
            || !is_bounded_metadata(&pack.name)
            || !is_bounded_metadata(&pack.version)
            || !matches!(pack.source_kind.as_str(), "builtin" | "file")
        {
            return Err(Refusal::ShapeInvalid);
        }
    }
    if inputs.limits.values().contains(&0) {
        return Err(Refusal::ValueOutOfRange);
    }
    let lint_consistent = match (inputs.lint.enabled, inputs.lint.max_results) {
        (true, Some(_)) => !inputs.packs.is_empty(),
        (false, None) => inputs.packs.is_empty(),
        _ => false,
    };
    if !lint_consistent {
        return Err(Refusal::InconsistentLintState);
    }
    let rendered = render_pretty(&inputs).map_err(|_| Refusal::ShapeInvalid)?;
    if rendered != bytes {
        return Err(Refusal::NotCanonical);
    }
    Ok(inputs)
}

/// Validate report bytes: known claim ids only, no duplicate ids, and exactly the
/// bytes the existing renderer produces for them. Claim-set completeness and order
/// are deliberately not checked.
pub(super) fn validate_report(bytes: &[u8]) -> Result<(), Refusal> {
    scan_structure(bytes)?;
    let report: TrustBasis = serde_json::from_slice(bytes).map_err(|_| Refusal::ShapeInvalid)?;
    if !duplicate_trust_basis_claim_ids(&report).is_empty() {
        return Err(Refusal::DuplicateClaimId);
    }
    let rendered = to_canonical_json_bytes(&report).map_err(|_| Refusal::ShapeInvalid)?;
    if rendered != bytes {
        return Err(Refusal::NotCanonical);
    }
    Ok(())
}

pub(super) fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Why a freshly built pair is not one the reader would bind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum PairRefusal {
    TooLarge(Member),
    Sidecar(Refusal),
    Report(Refusal),
    ReportMismatch,
}

impl PairRefusal {
    pub(super) fn code(self) -> String {
        match self {
            Self::TooLarge(_) => "artifact_set: member_too_large".to_string(),
            Self::Sidecar(refusal) => format!("sidecar_contract: {}", refusal.code()),
            Self::Report(refusal) => format!("report_contract: {}", refusal.code()),
            Self::ReportMismatch => "report_binding: digest_mismatch".to_string(),
        }
    }
}

/// Validate a report and sidecar as the reader does, in the reader's order, without
/// touching the file system. The writer calls this before it creates any output.
pub(super) fn validate_pair(report: &[u8], sidecar: &[u8]) -> Result<InputsV0, PairRefusal> {
    for (member, bytes) in [(Member::Report, report), (Member::Sidecar, sidecar)] {
        if !member.admits(bytes.len() as u64) {
            return Err(PairRefusal::TooLarge(member));
        }
    }
    let inputs = validate_inputs(sidecar).map_err(PairRefusal::Sidecar)?;
    validate_report(report).map_err(PairRefusal::Report)?;
    if !identity_matches(&inputs.report, &sha256_hex(report), report.len() as u64) {
        return Err(PairRefusal::ReportMismatch);
    }
    Ok(inputs)
}

// ---------------------------------------------------------------- bounded member reads

enum MemberError {
    Missing,
    NotRegular,
    TooLarge,
    Unreadable,
}

fn classify_io(error: &io::Error) -> MemberError {
    if error.kind() == io::ErrorKind::NotFound {
        MemberError::Missing
    } else {
        MemberError::Unreadable
    }
}

/// Open a regular, non-symlink file. The type check precedes the open; a same-user
/// process that swaps the path in between is outside this reader's threat model.
fn open_regular(path: &Path) -> Result<File, MemberError> {
    let metadata = std::fs::symlink_metadata(path).map_err(|e| classify_io(&e))?;
    if !metadata.file_type().is_file() {
        return Err(MemberError::NotRegular);
    }
    let file = File::open(path).map_err(|e| classify_io(&e))?;
    let opened = file.metadata().map_err(|e| classify_io(&e))?;
    if !opened.is_file() {
        return Err(MemberError::NotRegular);
    }
    Ok(file)
}

/// Read a whole member, refusing it once it exceeds its ceiling.
fn read_member(path: &Path, member: Member) -> Result<Vec<u8>, MemberError> {
    let file = open_regular(path)?;
    let mut bytes = Vec::new();
    file.take(member.ceiling().saturating_add(1))
        .read_to_end(&mut bytes)
        .map_err(|e| classify_io(&e))?;
    if !member.admits(bytes.len() as u64) {
        return Err(MemberError::TooLarge);
    }
    Ok(bytes)
}

/// Hash a member without materialising it, refusing it once it exceeds `limit`.
fn hash_member(path: &Path, limit: u64) -> Result<(String, u64), MemberError> {
    let file = open_regular(path)?;
    let mut reader = file.take(limit.saturating_add(1));
    let mut hasher = Sha256::new();
    let mut count: u64 = 0;
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(classify_io(&e)),
        };
        hasher.update(&buffer[..read]);
        count += read as u64;
    }
    if count > limit {
        return Err(MemberError::TooLarge);
    }
    Ok((hex::encode(hasher.finalize()), count))
}

// ---------------------------------------------------------------- check record

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum OverallStatus {
    Bound,
    Incomplete,
    Invalid,
    Mismatch,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CheckStatus {
    Passed,
    Failed,
    NotRequested,
    NotEvaluated,
}

#[derive(Debug, Serialize)]
struct CheckEntry {
    name: &'static str,
    status: CheckStatus,
    reason: Option<&'static str>,
}

#[derive(Debug, Default, Serialize)]
struct Observed {
    sidecar_sha256: Option<String>,
    report_sha256: Option<String>,
    bundle_sha256: Option<String>,
}

#[derive(Debug, Serialize)]
pub(super) struct InputsCheckV0 {
    schema: &'static str,
    status: OverallStatus,
    observed: Observed,
    checks: Vec<CheckEntry>,
    not_established: [&'static str; 5],
}

const ARTIFACT_SET: usize = 0;
const SIDECAR_CONTRACT: usize = 1;
const REPORT_CONTRACT: usize = 2;
const REPORT_BINDING: usize = 3;
const BUNDLE_BINDING: usize = 4;

impl InputsCheckV0 {
    fn new(bundle_requested: bool) -> Self {
        let entry = |name, status| CheckEntry {
            name,
            status,
            reason: None,
        };
        Self {
            schema: CHECK_SCHEMA,
            status: OverallStatus::Bound,
            observed: Observed::default(),
            checks: vec![
                entry("artifact_set", CheckStatus::NotEvaluated),
                entry("sidecar_contract", CheckStatus::NotEvaluated),
                entry("report_contract", CheckStatus::NotEvaluated),
                entry("report_binding", CheckStatus::NotEvaluated),
                entry(
                    "bundle_binding",
                    if bundle_requested {
                        CheckStatus::NotEvaluated
                    } else {
                        CheckStatus::NotRequested
                    },
                ),
            ],
            not_established: NOT_ESTABLISHED,
        }
    }

    fn pass(&mut self, index: usize) {
        self.checks[index].status = CheckStatus::Passed;
    }

    /// Record the first failure. Every later check keeps `not_evaluated` (or
    /// `not_requested`): once one layer fails, nothing that depends on it is read.
    fn fail(mut self, index: usize, status: OverallStatus, reason: &'static str) -> Self {
        self.checks[index].status = CheckStatus::Failed;
        self.checks[index].reason = Some(reason);
        self.status = status;
        self
    }

    fn fail_member(self, index: usize, error: MemberError) -> Self {
        match error {
            MemberError::Missing => self.fail(index, OverallStatus::Incomplete, "member_missing"),
            MemberError::NotRegular => {
                self.fail(index, OverallStatus::Invalid, "member_not_regular")
            }
            MemberError::TooLarge => self.fail(index, OverallStatus::Invalid, "member_too_large"),
            MemberError::Unreadable => {
                self.fail(index, OverallStatus::Unavailable, "member_unreadable")
            }
        }
    }

    pub(super) fn status(&self) -> OverallStatus {
        self.status
    }

    pub(super) fn render_text(&self) -> String {
        let mut text = format!("Trust Basis inputs: {}", status_word(self.status));
        for check in &self.checks {
            text.push_str(&format!("\n  {}: {}", check.name, check_word(check.status)));
            if let Some(reason) = check.reason {
                text.push_str(&format!(" ({reason})"));
            }
        }
        text.push_str(&format!(
            "\n  not established: {}",
            self.not_established.join(", ")
        ));
        text
    }
}

fn status_word(status: OverallStatus) -> &'static str {
    match status {
        OverallStatus::Bound => "bound",
        OverallStatus::Incomplete => "incomplete",
        OverallStatus::Invalid => "invalid",
        OverallStatus::Mismatch => "mismatch",
        OverallStatus::Unavailable => "unavailable",
    }
}

fn check_word(status: CheckStatus) -> &'static str {
    match status {
        CheckStatus::Passed => "passed",
        CheckStatus::Failed => "failed",
        CheckStatus::NotRequested => "not_requested",
        CheckStatus::NotEvaluated => "not_evaluated",
    }
}

fn contract_failure(check: InputsCheckV0, index: usize, refusal: Refusal) -> InputsCheckV0 {
    check.fail(index, OverallStatus::Invalid, refusal.code())
}

/// Check one artifact directory, in the contract's order: member access, sidecar
/// contract, report contract, report binding, then the optional bundle binding.
pub(super) fn check_directory(dir: &Path, bundle: Option<&Path>) -> InputsCheckV0 {
    let mut check = InputsCheckV0::new(bundle.is_some());

    match std::fs::symlink_metadata(dir) {
        Ok(metadata) if metadata.file_type().is_dir() => {}
        Ok(_) => return check.fail_member(ARTIFACT_SET, MemberError::NotRegular),
        Err(error) => return check.fail_member(ARTIFACT_SET, classify_io(&error)),
    }
    let report_bytes = match read_member(&dir.join(REPORT_FILE), Member::Report) {
        Ok(bytes) => bytes,
        Err(error) => return check.fail_member(ARTIFACT_SET, error),
    };
    check.observed.report_sha256 = Some(sha256_hex(&report_bytes));
    let sidecar_bytes = match read_member(&dir.join(INPUTS_FILE), Member::Sidecar) {
        Ok(bytes) => bytes,
        Err(error) => return check.fail_member(ARTIFACT_SET, error),
    };
    check.observed.sidecar_sha256 = Some(sha256_hex(&sidecar_bytes));
    check.pass(ARTIFACT_SET);

    let inputs = match validate_inputs(&sidecar_bytes) {
        Ok(inputs) => inputs,
        Err(refusal) => return contract_failure(check, SIDECAR_CONTRACT, refusal),
    };
    check.pass(SIDECAR_CONTRACT);

    if let Err(refusal) = validate_report(&report_bytes) {
        return contract_failure(check, REPORT_CONTRACT, refusal);
    }
    check.pass(REPORT_CONTRACT);

    let report_bound = check
        .observed
        .report_sha256
        .as_deref()
        .is_some_and(|observed| {
            identity_matches(&inputs.report, observed, report_bytes.len() as u64)
        });
    if !report_bound {
        return check.fail(REPORT_BINDING, OverallStatus::Mismatch, "digest_mismatch");
    }
    check.pass(REPORT_BINDING);

    let Some(bundle) = bundle else {
        return check;
    };
    // The reader's own retained-bundle ceiling, never one read from the sidecar.
    let ceiling = VerifyLimits::for_retained_events().max_bundle_bytes;
    let (bundle_sha256, bundle_bytes) = match hash_member(bundle, ceiling) {
        Ok(identity) => identity,
        Err(error) => return check.fail_member(BUNDLE_BINDING, error),
    };
    let bundle_bound = identity_matches(&inputs.bundle, &bundle_sha256, bundle_bytes);
    check.observed.bundle_sha256 = Some(bundle_sha256);
    if !bundle_bound {
        return check.fail(BUNDLE_BINDING, OverallStatus::Mismatch, "digest_mismatch");
    }
    check.pass(BUNDLE_BINDING);
    check
}

#[cfg(test)]
mod tests {
    use super::*;
    use assay_evidence::{
        TrustBasisClaim, TrustClaimBoundary, TrustClaimId, TrustClaimLevel, TrustClaimSource,
    };

    #[test]
    fn scan_refuses_a_duplicate_key_in_an_object_inside_an_array() {
        let document = br#"{"a":[{"b":1},{"c":1,"c":1}]}"#;
        assert_eq!(scan_structure(document), Err(Refusal::DuplicateKey));
    }

    #[test]
    fn scan_admits_the_largest_wire_integer_and_refuses_the_next() {
        assert_eq!(scan_structure(b"[9007199254740991]"), Ok(()));
        assert_eq!(
            scan_structure(b"[9007199254740992]"),
            Err(Refusal::NumberInvalid)
        );
        // Larger than u64: serde_json reads it as a float, which is refused too.
        assert_eq!(
            scan_structure(b"[100000000000000000000]"),
            Err(Refusal::NumberInvalid)
        );
    }

    #[test]
    fn scan_counts_depth_from_the_outermost_value() {
        let at_limit = format!("{}{}", "[".repeat(16), "]".repeat(16));
        let over_limit = format!("{}{}", "[".repeat(17), "]".repeat(17));
        assert_eq!(scan_structure(at_limit.as_bytes()), Ok(()));
        assert_eq!(
            scan_structure(over_limit.as_bytes()),
            Err(Refusal::DepthExceeded)
        );
    }

    #[test]
    fn scan_refuses_trailing_content_as_malformed() {
        assert_eq!(scan_structure(b"{} {}"), Err(Refusal::MalformedJson));
    }

    #[test]
    fn a_negative_zero_is_refused_as_a_number_that_is_not_a_wire_integer() {
        // serde_json reads `-0` as a float, so the scan refuses it before any
        // typed parse could normalise it to zero.
        assert_eq!(scan_structure(b"[-0]"), Err(Refusal::NumberInvalid));
    }

    // ------------------------------------------------------------ member ceilings

    /// A canonical report whose rendered length is exactly `size`, tuned through the
    /// length of one claim's note.
    fn report_of_size(size: u64) -> Vec<u8> {
        let render = |note_len: usize| {
            to_canonical_json_bytes(&TrustBasis {
                claims: vec![TrustBasisClaim {
                    id: TrustClaimId::BundleVerified,
                    level: TrustClaimLevel::Verified,
                    source: TrustClaimSource::BundleVerification,
                    boundary: TrustClaimBoundary::BundleWide,
                    note: Some("n".repeat(note_len)),
                }],
            })
            .unwrap()
        };
        let base = render(0).len() as u64;
        let report = render((size - base) as usize);
        assert_eq!(report.len() as u64, size);
        report
    }

    fn pack(name: String, version: String) -> PackV0 {
        PackV0 {
            name,
            version,
            source_kind: "file".to_string(),
            digest: format!("sha256:{}", "0".repeat(64)),
        }
    }

    fn inputs_for(report: &[u8], packs: Vec<PackV0>) -> InputsV0 {
        InputsV0 {
            schema: INPUTS_SCHEMA.to_string(),
            report: ByteIdentityV0::of(report),
            bundle: ByteIdentityV0::of(b"bundle"),
            pack_digest_domain: PACK_DIGEST_DOMAIN.to_string(),
            lint: LintV0 {
                enabled: !packs.is_empty(),
                max_results: (!packs.is_empty()).then_some(0),
            },
            packs,
            limits: LimitsV0::from_limits(&VerifyLimits::for_retained_events()),
            reported_assay_version: "test".to_string(),
        }
    }

    /// A canonical, otherwise valid sidecar whose rendered length is exactly `size`.
    /// Filler packs carry 256 bytes of U+0001 (six bytes each once escaped); one last
    /// pack's name and version absorb the remainder, so every metadata field stays
    /// within the 256-byte ceiling and the pack count within 64.
    fn sidecar_of_size(report: &[u8], size: u64) -> Vec<u8> {
        let escaped = "\u{1}".repeat(256);
        for fillers in 0..MAX_PACKS {
            let mut packs = vec![pack("f".to_string(), escaped.clone()); fillers];
            packs.push(pack("a".to_string(), "v".to_string()));
            let base = render_pretty(&inputs_for(report, packs.clone()))
                .unwrap()
                .len() as u64;
            if base > size {
                break;
            }
            let needed = size - base;
            // Version: `len` bytes of which `esc` are U+0001 adds len - 1 + 5 * esc.
            for len in 1..=256u64 {
                for esc in 0..=len {
                    let added = len - 1 + 5 * esc;
                    if added > needed || needed - added > 255 {
                        continue;
                    }
                    let name_len = 1 + (needed - added) as usize;
                    let version = format!(
                        "{}{}",
                        "\u{1}".repeat(esc as usize),
                        "v".repeat((len - esc) as usize)
                    );
                    *packs.last_mut().unwrap() = pack("a".repeat(name_len), version);
                    let sidecar = render_pretty(&inputs_for(report, packs.clone())).unwrap();
                    assert_eq!(sidecar.len() as u64, size);
                    return sidecar;
                }
            }
        }
        panic!("no canonical sidecar of {size} bytes");
    }

    /// The writer's verdict on a pair and the reader's verdict on the same bytes on
    /// disk, side by side. They must agree for every case below.
    fn writer_and_reader(
        report: &[u8],
        sidecar: &[u8],
    ) -> (Result<(), PairRefusal>, InputsCheckV0) {
        let root = tempfile::tempdir().unwrap();
        let dir = root.path().join("set");
        std::fs::create_dir(&dir).unwrap();
        std::fs::write(dir.join(REPORT_FILE), report).unwrap();
        std::fs::write(dir.join(INPUTS_FILE), sidecar).unwrap();
        (
            validate_pair(report, sidecar).map(|_| ()),
            check_directory(&dir, None),
        )
    }

    #[test]
    fn the_writer_admits_a_sidecar_exactly_as_far_as_the_reader_does() {
        let report = report_of_size(2048);
        for (size, admitted) in [(65_535, true), (65_536, true), (65_537, false)] {
            let sidecar = sidecar_of_size(&report, size);
            assert!(
                validate_inputs(&sidecar).is_ok(),
                "{size}: content is valid"
            );
            let (writer, reader) = writer_and_reader(&report, &sidecar);
            if admitted {
                assert_eq!(writer, Ok(()), "{size}");
                assert_eq!(reader.status(), OverallStatus::Bound, "{size}");
            } else {
                assert_eq!(
                    writer,
                    Err(PairRefusal::TooLarge(Member::Sidecar)),
                    "{size}"
                );
                assert_eq!(reader.status(), OverallStatus::Invalid, "{size}");
                assert_eq!(reader.checks[ARTIFACT_SET].reason, Some("member_too_large"));
            }
        }
    }

    #[test]
    fn the_writer_admits_a_report_exactly_as_far_as_the_reader_does() {
        for (size, admitted) in [(1_048_575, true), (1_048_576, true), (1_048_577, false)] {
            let report = report_of_size(size);
            let sidecar = render_pretty(&inputs_for(&report, vec![])).unwrap();
            let (writer, reader) = writer_and_reader(&report, &sidecar);
            if admitted {
                assert_eq!(writer, Ok(()), "{size}");
                assert_eq!(reader.status(), OverallStatus::Bound, "{size}");
            } else {
                assert_eq!(writer, Err(PairRefusal::TooLarge(Member::Report)), "{size}");
                assert_eq!(reader.status(), OverallStatus::Invalid, "{size}");
                assert_eq!(reader.checks[ARTIFACT_SET].reason, Some("member_too_large"));
            }
        }
    }

    #[test]
    fn size_is_refused_before_content_and_the_report_before_the_sidecar() {
        // The reader reads the report, then the sidecar, before parsing either.
        let big_report = report_of_size(MAX_REPORT_BYTES + 1);
        let big_sidecar = vec![b' '; (MAX_SIDECAR_BYTES + 1) as usize];
        assert_eq!(
            validate_pair(&big_report, &big_sidecar).unwrap_err(),
            PairRefusal::TooLarge(Member::Report)
        );
        assert_eq!(
            validate_pair(&report_of_size(2048), &big_sidecar).unwrap_err(),
            PairRefusal::TooLarge(Member::Sidecar),
            "an oversized sidecar is refused for its size, not as malformed JSON"
        );
    }
}
