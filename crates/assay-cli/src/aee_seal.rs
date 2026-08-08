//! ADR-045 Landlock run-end seal: eligibility and payload assembly.
//!
//! This is the producer side of the primitive whose checker landed in #2006. It deliberately stops
//! short of signing. ADR-045 authorises "only the primitive design and fixture/checker work, not
//! stable production AEE export" until the production signing envelope — format, algorithm, key
//! role, canonical bytes rule, duplicate-member rejection — is chosen and tested. Assembling a
//! payload is inside that boundary; signing one is not.
//!
//! The property this module exists to hold is narrow and is the reason it refuses rather than
//! returns an `Option`: **a run that cannot prove its seal values must emit no seal at all**, not a
//! seal carrying guessed ones. ADR-045 says so in as many words for drop accounting — "It MUST NOT
//! emit an AEE-looking seal with guessed zero drop accounting" — and the same reasoning covers
//! still-armed state. A seal is the strongest record in an AEE statement; a guessed one is worse
//! than a missing one, because a missing seal is refused by the checker and a guessed seal is not.

use crate::enforcement_health_v1::{EnforcementHealthV1, Mechanism, Probe, Status};

/// The collection path this slice covers. ADR-045 chose Option C: Landlock first, proxy later.
/// The Landlock connect path. One value of a set, not the set.
///
/// This is what `assay sandbox` passes today. It is a constant rather than a literal so a caller
/// names a path this crate recognises, and it is no longer read inside the emit functions: the
/// consumer side has always treated the path as one of a set — `TrustedObservationKey` carries
/// `collection_paths: Vec<String>` and `check_substrate_scope` asks `contains(&path)` — while the
/// producer could only ever name one. That asymmetry was a capability gap, not a design position.
pub const COLLECTION_PATH_LANDLOCK_TCP_CONNECT: &str = "landlock-tcp-connect";
/// The second collection path (#2093): the JSON-RPC enforcing proxy.
pub const COLLECTION_PATH_JSONRPC_PROXY: &str = "jsonrpc-proxy";
/// The carrier profile that establishes proxy enforcement, reported as `assaySourceSchema`.
pub const PROXY_SOURCE_SCHEMA: &str = "assay.enforcement_decision.v0";
/// What a proxy seal covers.
pub const PROXY_SEAL_SCOPE: &str = "tool_call:mcp_proxy_policy";
/// AEE draft version this payload is shape-compatible with. Not a conformance claim.
///
/// **Stays at 0.7, and there is no 0.8 to wait for.** Decided in #2093; the reason was corrected
/// after the predicate author answered on in-toto/attestation#570 (2026-08-07).
///
/// This first read his earlier sentence as an offer to ship 0.7 and repair the referencedness
/// defect in 0.8, and recorded a migration trigger accordingly. He has since said to pin 0.7 and
/// not plan for 0.8: the repair already landed on that branch as `c0c4da6` on 4 August, inside the
/// 0.7 changelog rather than behind a bump, so that nobody has to pin a version whose reject family
/// one edit empties.
///
/// So this constant tracks whatever version #570 carries. The thing to watch is that branch, not a
/// successor version. See ADR-045 for the full correction.
///
/// `tests/aee_version_parity.rs` pins this against the fixture checker's own copy. Two literals
/// for one version drift silently, and a bump that reaches only one side would leave a producer
/// emitting a version its own checker rejects.
pub const AEE_VERSION: &str = "0.7";
/// The only drop-accounting proof model the Landlock-first slice can honestly carry.
pub const DROP_PROOF_SYNCHRONOUS_PROBE: &str = "synchronous-probe";
/// The other model ADR-045 names: every channel exposes a loss counter and each read zero.
pub const DROP_PROOF_COUNTED_QUEUE_ZERO: &str = "counted-queue-zero";

/// Landlock denies `connect(2)` with `EACCES`. ADR-045 and `enforcement_health.v1` agree that weak
/// signals never count as a block: a timeout or `ECONNREFUSED` can be produced by a host that is
/// simply not listening, which is indistinguishable from enforcement that was never armed.
const BLOCKING_ERRNO: &str = "EACCES";

/// `LANDLOCK_ACCESS_NET_CONNECT_TCP` was introduced in Landlock ABI 4 (Linux 6.7). A kernel below
/// that cannot express a TCP-connect restriction at all.
const LANDLOCK_ABI_NET_CONNECT_TCP: u32 = 4;

/// The one `restriction_shedding` value that lets a seal claim `aeeStillArmed`.
///
/// Same spelling as `backend::SheddingProbeOutcome::label`, and only that value: `inconclusive`
/// means the probe found nothing, which is not the same as finding that restrictions hold.
const RESTRICTIONS_HELD: &str = "restrictions_held";

/// The operator-facing label for what this probe observed.
fn observed_label(probe: &Probe) -> String {
    if probe.listener_reached {
        "no_connect_block".to_string()
    } else {
        "connect_blocked".to_string()
    }
}

/// Why a run cannot carry a production seal.
///
/// Each variant names one seal-eligibility condition from ADR-045. They are separate variants
/// rather than one string because a caller that logs "not eligible" and a caller that decides
/// whether to retry need different answers, and because a test asserting the reason is the only
/// kind that proves the refusal happened for the reason it was written for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NotSealEligible {
    /// Enforcement was requested but never installed, so nothing was ever armed.
    NotArmed { status: Status },
    /// A mechanism other than Landlock produced this record; this slice covers one path.
    WrongMechanism { mechanism: Mechanism },
    /// The scope in the health record is not the one this seal claims.
    ScopeMismatch { found: String },
    /// No run-end probe. ADR-045: start-time `restrict_self_confirmed` alone is not sufficient,
    /// because it proves the ruleset was applied at start, not that it was still applied at end.
    NoRunEndProbe,
    /// A probe ran and the listener was reached, so the connect was not blocked.
    ProbeReachedListener,
    /// A probe ran but reported a signal that does not distinguish enforcement from an absent
    /// listener.
    ProbeSignalTooWeak { errno: String },
    /// The kernel cannot express the restriction whose denial is being claimed, so an `EACCES`
    /// here did not come from Landlock. `LANDLOCK_ACCESS_NET_CONNECT_TCP` exists from ABI 4.
    AbiCannotExpressRestriction { abi: u32 },
    /// The record's own consistency is broken: it reports an applied ruleset and a failure, or
    /// claims a restriction it also reports as unsupported.
    RecordSelfContradictory { detail: String },
    /// A run-context value the seal would carry is not a value this run proved.
    UnprovenContextValue { field: &'static str },
    /// A digest the seal must derive could not be computed from the inputs given.
    DerivationFailed { what: &'static str },
    /// The run did not establish that Landlock restrictions cannot be shed on this kernel, so
    /// `aeeStillArmed` would rest on an invariant this run has no evidence for.
    ///
    /// ADR-045 lets still-armed rest on "a documented kernel-level invariant showing the applied
    /// Landlock restrictions cannot be relaxed". CVE-2024-42318 is that invariant's exception --
    /// `keyctl(KEYCTL_SESSION_TO_PARENT)` shed every restriction from 5.13 until v6.11-rc1 -- and
    /// `abi >= 4` does not exclude it, because ABI 4 arrives four releases earlier at 6.7.
    RestrictionSheddingNotEstablished { found: String },
    /// A counted-queue channel reported lost observations, so zero cannot be carried.
    ObservationsLost { channel: String, lost: u64 },
    /// A counted-queue model was declared with no channels, which proves nothing.
    DropAccountingUnnamed,
}

impl NotSealEligible {
    /// Stable reason slug, for logs and for tests that must assert the specific refusal.
    #[must_use]
    pub fn code(&self) -> &'static str {
        match self {
            Self::NotArmed { .. } => "not-armed",
            Self::WrongMechanism { .. } => "wrong-mechanism",
            Self::ScopeMismatch { .. } => "scope-mismatch",
            Self::NoRunEndProbe => "no-run-end-probe",
            Self::ProbeReachedListener => "probe-reached-listener",
            Self::ProbeSignalTooWeak { .. } => "probe-signal-too-weak",
            Self::AbiCannotExpressRestriction { .. } => "abi-cannot-express-restriction",
            Self::RecordSelfContradictory { .. } => "record-self-contradictory",
            Self::UnprovenContextValue { .. } => "unproven-context-value",
            Self::DerivationFailed { .. } => "derivation-failed",
            Self::RestrictionSheddingNotEstablished { .. } => {
                "restriction-shedding-not-established"
            }
            Self::ObservationsLost { .. } => "observations-lost",
            Self::DropAccountingUnnamed => "drop-accounting-unnamed",
        }
    }
}

impl std::fmt::Display for NotSealEligible {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotArmed { status } => write!(f, "enforcement is {status:?}, so nothing was armed for this run"),
            Self::WrongMechanism { mechanism } => write!(f, "mechanism is {mechanism:?}; this seal covers Landlock only"),
            Self::ScopeMismatch { found } => write!(f, "health record scope {found:?} is not the sealed scope"),
            Self::NoRunEndProbe => write!(f, "no run-end probe: a start-time restrict_self confirmation proves the ruleset was applied, not that it was still applied at run end"),
            Self::ProbeReachedListener => write!(f, "the run-end probe reached the listener, so the connect was not blocked"),
            Self::ProbeSignalTooWeak { errno } => write!(f, "the run-end probe reported {errno:?}, which does not distinguish enforcement from an absent listener"),
            Self::AbiCannotExpressRestriction { abi } => write!(f, "Landlock ABI {abi} predates LANDLOCK_ACCESS_NET_CONNECT_TCP (ABI 4), so this denial did not come from Landlock"),
            Self::RecordSelfContradictory { detail } => write!(f, "health record contradicts itself: {detail}"),
            Self::UnprovenContextValue { field } => write!(f, "run-context field {field} is not a value this run proved"),
            Self::DerivationFailed { what } => write!(f, "could not derive a required digest: {what}"),
            Self::RestrictionSheddingNotEstablished { found } => write!(
                f,
                "restriction-shedding was not established for this kernel ({found}); aeeStillArmed would rest on an invariant CVE-2024-42318 shows has an exception, and the Landlock ABI does not exclude it"
            ),
            Self::ObservationsLost { channel, lost } => write!(f, "channel {channel} lost {lost} observations, so zero drop accounting cannot be carried"),
            Self::DropAccountingUnnamed => write!(f, "a counted-queue model with no channels names no proof at all"),
        }
    }
}

/// Lowercase SHA-256 hex, the shape the #2001 field contract requires of every digest member.
pub(crate) fn is_sha256_hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `YYYY-MM-DDTHH:MM:SSZ`, the shape the ADR-045 checker's validity-window check parses.
fn is_rfc3339_utc(v: &str) -> bool {
    let b = v.as_bytes();
    if b.len() != 20
        || b[4] != b'-'
        || b[7] != b'-'
        || b[10] != b'T'
        || b[13] != b':'
        || b[16] != b':'
        || b[19] != b'Z'
        || ![0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
    {
        return false;
    }
    // Shape alone accepted `2026-02-30`, `9999-99-99` and `24:00:00`, which the checker's `strptime`
    // refuses. An unparsable instant is worse than a mismatched one: it makes the checker skip the
    // whole key-validity-window loop, so the bad value ends up named in a message about a check that
    // did not run.
    let num = |a: usize, z: usize| v[a..z].parse::<u32>().unwrap_or(u32::MAX);
    let (year, month, day) = (num(0, 4), num(5, 7), num(8, 10));
    let (hour, minute, second) = (num(11, 13), num(14, 16), num(17, 19));
    // `strptime` has no year 0, so accepting one emits an instant the checker cannot parse -- and an
    // unparsable instant makes it skip the whole key-validity-window loop.
    if year < 1 {
        return false;
    }
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return false,
    };
    (1..=days_in_month).contains(&day) && hour < 24 && minute < 60 && second < 60
}

/// The ADR-045 sealed payload, assembled but **not signed**.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct SealPayload {
    #[serde(rename = "aeeKind")]
    pub aee_kind: String,
    #[serde(rename = "aeeVersion")]
    pub aee_version: String,
    #[serde(rename = "aeeRunBinding")]
    pub aee_run_binding: String,
    #[serde(rename = "aeeMethod")]
    pub aee_method: String,
    #[serde(rename = "aeePostureDigest")]
    pub aee_posture_digest: String,
    #[serde(rename = "aeeStillArmed")]
    pub aee_still_armed: bool,
    #[serde(rename = "aeeDropCount")]
    pub aee_drop_count: u64,
    #[serde(rename = "aeeDropBound")]
    pub aee_drop_bound: u64,
    #[serde(rename = "aeeObservedSet")]
    pub aee_observed_set: String,
    #[serde(rename = "aeeObservedAttacks")]
    pub aee_observed_attacks: Vec<String>,
    #[serde(rename = "assayObservedLabels")]
    pub assay_observed_labels: Vec<String>,
    #[serde(rename = "assayCollectionPath")]
    pub assay_collection_path: String,
    #[serde(rename = "assaySealedAt")]
    pub assay_sealed_at: String,
    #[serde(rename = "assaySourceSchema")]
    pub assay_source_schema: String,
    #[serde(rename = "assaySealScope")]
    pub assay_seal_scope: String,
    #[serde(rename = "assayDropProofModel")]
    pub assay_drop_proof_model: String,
    /// `checked` or `declared` -- whether anything was verified about the model above.
    #[serde(rename = "assayDropProofBasis")]
    pub assay_drop_proof_basis: String,
    /// `name=lost` per channel, empty when the basis is `declared`. The readings the claim rests on.
    #[serde(rename = "assayDropChannels")]
    pub assay_drop_channels: Vec<String>,
    #[serde(rename = "assayAttackRowAttributionSource")]
    pub assay_attack_row_attribution_source: String,
    #[serde(rename = "assayNonClaims")]
    pub assay_non_claims: Vec<String>,
}

/// The payload-local minimum non-claims from the #2001 payload contract.
/// The standing non-claims every seal carries.
///
/// # The fifth one is a ceiling, not a coverage gap
///
/// "does not distinguish withdrawn coverage from coverage never held" breaks the `does not prove`
/// pattern of its four siblings on purpose. Those name things this seal did not establish and a
/// better producer might. This one names something **no** producer can establish, and phrasing it
/// as a proof gap would understate it as work someone could do.
///
/// The measurement is the predicate author's, reported on in-toto/attestation#570: a completed
/// coverage withdrawal is byte-identical to an honest producer with no coverage, across 27 of 27
/// baselines on three independent rails, and the honest form of the hedge is that it is not
/// detectable by any check anyone can write.
///
/// It looks like log truncation and the analogy is worth following until it breaks. Against a
/// tag-in-the-clear scheme an adversary emits a prefix with that prefix's own valid tag, which no
/// verifier can tell from an honest shorter log (arXiv 2509.03821 gives the game). The defence
/// there is to make the earlier tag unrecoverable -- which works because the adversary is not the
/// tag holder. **Here the producer holds the signing key.** An operator emitting less coverage
/// forges nothing; every seal is validly signed. No integrity mechanism inside the artifact can
/// separate an authorised signer emitting less from an authorised signer with less to emit.
///
/// So this is our own occurrence-versus-absence rule, met at its boundary.
/// `assay_runner_schema::claim_parity` states it as "absence is never looser than occurrence", and
/// `partial_coverage_blocks_absence_claim` enforces it elsewhere in the tree. A seal is occurrence
/// evidence: it reports that a run was armed and that something was refused. This non-claim is the
/// formal reason it can never be read as absence evidence, and it is stated here rather than
/// invented as a new hedge because the seal is an instance of a rule we already hold.
fn payload_non_claims() -> Vec<String> {
    [
        "does not prove complete run population",
        "does not prove agent safety",
        "does not prove provider side effects",
        "does not prove independent substrate operation",
        "does not distinguish withdrawn coverage from coverage never held",
    ]
    .iter()
    .map(|s| (*s).to_string())
    .collect()
}

/// The observation environment the run binding is derived from.
///
/// These are the run's pre-injection inputs. The seal does not accept a run binding; it derives one
/// from these the same way the checker does, because a digest a producer is handed is a digest
/// nobody proved.
#[derive(Debug, Clone)]
pub struct ObservationEnvironment {
    pub subject_digest: String,
    pub substrate_digest: String,
    pub corpus_digest: String,
    pub catch_policy_digest: String,
    pub observation_vocabulary_digest: String,
    pub run_entropy_digest: String,
    /// The carried posture object. Digested here, not supplied as a digest.
    pub network_posture: serde_json::Value,
}

/// One emitted observation record, as it appears in `observationRecords`.
///
/// Serializable because the seal commits to these and a consumer cannot check that commitment
/// without them (#2135). `aeeObservedSet` is a digest over the interception and examination leaves;
/// emitting the digest while withholding the leaves leaves a member only its own producer can
/// re-derive, which is the party it exists to constrain.
///
/// Field names are the wire names, not the Rust ones: `payloadType` is what DSSE calls it and what
/// an `observationRecords` entry carries.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ObservationRecord {
    pub payload: serde_json::Value,
    #[serde(rename = "payloadType")]
    pub payload_type: String,
}

/// AEE binding version this producer derives under.
const AEE_BINDING_VERSION: &str = "2";
/// The payload type Assay's fixture-era observation records carry.
pub const OBSERVATION_PAYLOAD_TYPE: &str =
    "application/vnd.assay.aee-landlock-seal.fixture.v0+json";

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// RFC 8785 canonical bytes, then SHA-256. The one canonicalizer, from `assay-canonical`.
/// The declared digest of a carried object, for a producer building a posture.
///
/// Public because the producer lives in `sandbox/child.rs` and must compute the posture's *declared*
/// digest with the same canonicalization the seal derives its own digests with. A second
/// implementation there would be a second answer to "what is this object's digest", which is the
/// one question a run binding cannot have two answers to.
///
/// Falls back to the empty-object digest only if canonicalization fails, which cannot happen for a
/// posture built from literals; `build_sealed_run` rejects a posture whose declared digest is not
/// 64 hex characters, so a degenerate value is refused rather than sealed.
pub fn digest_json_public(value: &serde_json::Value) -> String {
    digest_json(value).unwrap_or_default()
}

/// The current instant in the shape `is_rfc3339_utc` accepts.
///
/// Seconds precision, `Z`, no fractional part: the ADR-045 checker's validity-window check parses
/// exactly `YYYY-MM-DDTHH:MM:SSZ`, and an instant it cannot parse is refused rather than compared.
pub fn now_rfc3339_utc() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn digest_json(value: &serde_json::Value) -> Result<String, NotSealEligible> {
    let bytes =
        assay_canonical::jcs::to_vec(value).map_err(|_| NotSealEligible::DerivationFailed {
            what: "canonicalize",
        })?;
    Ok(sha256_hex(&bytes))
}

/// AEE v0.7 leaf hash: `H(0x00 || the record's DSSE PAE bytes)`.
///
/// The `0x00` is the RFC 6962 leaf domain tag and is normative here; it is not ours to tidy away.
fn leaf_hash(rec: &ObservationRecord) -> Result<String, NotSealEligible> {
    let payload = assay_canonical::jcs::to_vec(&rec.payload).map_err(|_| {
        NotSealEligible::DerivationFailed {
            what: "canonicalize",
        }
    })?;
    let pae = assay_common::dsse::build_pae(&rec.payload_type, &payload);
    let mut buf = Vec::with_capacity(1 + pae.len());
    buf.push(0x00);
    buf.extend_from_slice(&pae);
    Ok(sha256_hex(&buf))
}

/// The records, as `--aee-records` writes them: one JSON object per line.
///
/// A function rather than a loop at the call site, because the call site is in `sandbox/child.rs`
/// behind `cfg(target_os = "linux")` and nothing on a non-Linux host compiles it, let alone runs
/// it. Biting the write path there changed nothing and no test noticed. The serialisation is the
/// part with a property worth holding -- a consumer must be able to parse back what was written --
/// so it lives where a test can reach it and the call site keeps only the file write.
///
/// The shape is **two members**, `payload` and `payloadType`. An `observationRecords` entry in a
/// statement carries `seq` and `signatures` as well, so this file is the digest's inputs and not a
/// statement fragment: enough for a consumer to recompute `aeeObservedSet`, not enough to feed the
/// fixture checker, which requires exactly one signature per record.
pub fn records_ndjson(records: &[ObservationRecord]) -> Result<String, serde_json::Error> {
    let mut out = String::new();
    for rec in records {
        out.push_str(&serde_json::to_string(rec)?);
        out.push('\n');
    }
    Ok(out)
}

/// AEE v0.7 `aeeObservedSet`: a digest over the sorted lowercase leaf hashes of every emitted
/// `interception` and `examination` record.
///
/// Dropping a record removes a leaf and the value diverges, so the commitment binds the carried set
/// against a party who cannot re-sign the envelope.
///
/// It does **not** establish when the seal was built. Every member of a leaf is a value the producer
/// chooses, so "the seal commits to this leaf" orders two computations inside one process, not two
/// events in the world. Ordering the probe against the run needs a party that is neither the
/// producer nor the assembly plane; nothing in this module provides one.
pub fn observed_set(records: &[ObservationRecord]) -> Result<String, NotSealEligible> {
    let mut leaves: Vec<String> = records
        .iter()
        .filter(|r| {
            matches!(
                r.payload.get("aeeKind").and_then(|v| v.as_str()),
                Some("interception") | Some("examination")
            )
        })
        .map(leaf_hash)
        .collect::<Result<Vec<_>, _>>()?;
    leaves.sort_unstable();
    leaves.dedup();
    digest_json(&serde_json::Value::Array(
        leaves.into_iter().map(serde_json::Value::String).collect(),
    ))
}

/// AEE v0.7 run binding over the run's pre-injection inputs.
pub fn run_binding(env: &ObservationEnvironment) -> Result<String, NotSealEligible> {
    digest_json(&serde_json::json!({
        "aeeBindingVersion": AEE_BINDING_VERSION,
        "catchPolicy": env.catch_policy_digest,
        "corpus": env.corpus_digest,
        "networkPosture": digest_json(&env.network_posture)?,
        "observationVocabulary": env.observation_vocabulary_digest,
        "runEntropy": env.run_entropy_digest,
        "subject": env.subject_digest,
        "substrate": env.substrate_digest,
    }))
}

/// Build the `examination` record that carries the run-end probe result.
///
/// The probe becomes a record rather than staying a field in a health artifact so that it travels:
/// a field in a side artifact reaches no consumer at all. That is the whole of the claim. It does
/// not establish the probe's position in the run — see `observed_set`.
pub fn probe_examination_record(
    probe: &Probe,
    run_binding: &str,
    collection_path: &str,
) -> ObservationRecord {
    ObservationRecord {
        payload: serde_json::json!({
            "aeeKind": "examination",
            "aeeVersion": AEE_VERSION,
            "aeeRunBinding": run_binding,
            "aeeMethod": "intercepted",
            "assayCollectionPath": collection_path,
            "assayProbeTransport": probe.transport,
            "assayProbeAction": probe.blocked_action,
            "assayProbePort": probe.blocked_port,
            "assayProbeErrno": probe.blocked_errno,
            "assayProbeListenerReached": probe.listener_reached,
        }),
        payload_type: OBSERVATION_PAYLOAD_TYPE.to_string(),
    }
}

/// The proxy vantage: what the JSON-RPC enforcing proxy can establish at run end.
///
/// The second collection path #2093 asks for. It is deliberately *not* an `EnforcementHealthV1`:
/// that type's `Mechanism` has one variant, and `seal_eligibility` gates on `health.landlock`, an
/// ABI number and a kernel shedding probe. A proxy has none of those, and shaping a Landlock record
/// to fit would be the exact class of claim this seal exists to refuse.
///
/// The inputs are the carriers the enforcing proxy already ships under the `privileged-mcp-action/v0`
/// profile: `assay.enforcement_decision.v0` says the proxy was deciding, and
/// `assay.denied_call_observation.v0` is the run-end refusal — the proxy's analogue of the Landlock
/// probe's `EACCES`.
#[derive(Debug, Clone)]
pub struct ProxyEnforcement {
    /// The proxy was loaded and deciding when the run ended.
    pub enforcing: bool,
    /// A failure the record itself carries. Present alongside `enforcing` is self-contradictory.
    pub failure: Option<String>,
    /// The run-end refusal, absent when the proxy denied nothing.
    pub denial: Option<ProxyDenial>,
}

/// One caller-visible denial observed by the proxy.
#[derive(Debug, Clone)]
pub struct ProxyDenial {
    /// The tool whose call was refused.
    pub tool: String,
    /// The reason code the proxy returned to the caller.
    pub reason_code: String,
    /// Whether the call nevertheless reached the upstream server. True refutes the denial, the way
    /// `listener_reached` refutes a Landlock block.
    pub upstream_reached: bool,
}

/// What the seal requires of *any* vantage, stated once.
///
/// Extracted because it is the rule, and the rule is not about Landlock: enforcement was armed at
/// run end, the record does not contradict itself, and something was actually refused. Each vantage
/// adapts its own evidence into these three questions and adds whatever else its mechanism can
/// establish. Landlock adds an ABI capability check and a kernel shedding probe; the proxy cannot,
/// and says so rather than implying otherwise.
fn armed_and_self_consistent(
    armed: bool,
    status: Status,
    failure_recorded: bool,
) -> Result<(), NotSealEligible> {
    if !armed {
        return Err(NotSealEligible::NotArmed { status });
    }
    if failure_recorded {
        return Err(NotSealEligible::RecordSelfContradictory {
            detail: "status is active and a failure is recorded".into(),
        });
    }
    Ok(())
}

/// Is this proxy run sealable?
///
/// The three shared questions, and no more. What it deliberately does **not** assert is the property
/// `restriction_shedding` establishes for Landlock: that enforcement could not be shed between
/// arming and run end. A kernel ruleset survives the process that installed it; an in-process proxy
/// is bypassed by not going through it. That asymmetry is real, it is not fixable by more checks
/// here, and the seal carries it as a non-claim on this path rather than letting a reader assume the
/// two vantages establish the same thing.
pub fn proxy_seal_eligibility(e: &ProxyEnforcement) -> Result<&ProxyDenial, NotSealEligible> {
    armed_and_self_consistent(
        e.enforcing,
        if e.enforcing {
            Status::Active
        } else {
            Status::Failed
        },
        e.failure.is_some(),
    )?;
    let denial = e.denial.as_ref().ok_or(NotSealEligible::NoRunEndProbe)?;
    if denial.upstream_reached {
        return Err(NotSealEligible::ProbeReachedListener);
    }
    if denial.reason_code.is_empty() {
        return Err(NotSealEligible::ProbeSignalTooWeak {
            errno: "<empty reason code>".to_string(),
        });
    }
    Ok(denial)
}

/// The proxy's examination record.
///
/// A separate constructor rather than a generalised one, because the Landlock payload is pinned
/// byte-for-byte by `derivation-parity.json` and the fixture checker: generalising the shared
/// builder would put those under a refactor for no gain. The two records answer the same question
/// in their own vocabularies, which is what `assayCollectionPath` exists to distinguish.
pub fn proxy_examination_record(
    denial: &ProxyDenial,
    run_binding: &str,
    collection_path: &str,
) -> ObservationRecord {
    ObservationRecord {
        payload: serde_json::json!({
            "aeeKind": "examination",
            "aeeVersion": AEE_VERSION,
            "aeeRunBinding": run_binding,
            "aeeMethod": "intercepted",
            "assayCollectionPath": collection_path,
            "assayDeniedTool": denial.tool,
            "assayDenialReasonCode": denial.reason_code,
            "assayDenialUpstreamReached": denial.upstream_reached,
        }),
        payload_type: OBSERVATION_PAYLOAD_TYPE.to_string(),
    }
}

pub fn seal_eligibility(health: &EnforcementHealthV1) -> Result<&Probe, NotSealEligible> {
    armed_and_self_consistent(
        health.status == Status::Active,
        health.status,
        health.failure.is_some(),
    )?;
    if health.mechanism != Mechanism::Landlock {
        return Err(NotSealEligible::WrongMechanism {
            mechanism: health.mechanism,
        });
    }
    if health.scope != crate::enforcement_health_v1::SCOPE_TCP_CONNECT_LANDLOCK_PORT {
        return Err(NotSealEligible::ScopeMismatch {
            found: health.scope.clone(),
        });
    }
    // `LANDLOCK_ACCESS_NET_CONNECT_TCP` exists from ABI 4. Below it the kernel cannot express the
    // restriction whose denial is claimed, so the EACCES came from somewhere that is not Landlock.
    if health.landlock.abi < LANDLOCK_ABI_NET_CONNECT_TCP {
        return Err(NotSealEligible::AbiCannotExpressRestriction {
            abi: health.landlock.abi,
        });
    }
    if health.landlock.net_connect_tcp_supported == Some(false) {
        return Err(NotSealEligible::RecordSelfContradictory {
            detail: "claims a TCP-connect restriction the record reports as unsupported".into(),
        });
    }
    if !health.landlock.no_new_privs_confirmed || !health.landlock.restrict_self_confirmed {
        return Err(NotSealEligible::RecordSelfContradictory {
            detail: "status is active but the ruleset was never confirmed applied".into(),
        });
    }
    // Measured, not inferred. `restrict_self_confirmed` above says the child was armed at start;
    // still-armed at end follows only if this kernel cannot shed restrictions, and that is a
    // property of the running kernel rather than of the ABI number.
    // `backend::landlock_shedding_probe` asks it directly, so no per-distribution backport table
    // has to be kept correct here -- and a table of backport facts is a thing that rots silently.
    match health.landlock.restriction_shedding.as_deref() {
        Some(RESTRICTIONS_HELD) => {}
        other => {
            return Err(NotSealEligible::RestrictionSheddingNotEstablished {
                found: other.unwrap_or("not measured").to_string(),
            })
        }
    }
    let probe = health
        .probe
        .as_ref()
        .ok_or(NotSealEligible::NoRunEndProbe)?;
    if probe.listener_reached {
        return Err(NotSealEligible::ProbeReachedListener);
    }
    if probe.blocked_errno != BLOCKING_ERRNO {
        return Err(NotSealEligible::ProbeSignalTooWeak {
            errno: probe.blocked_errno.clone(),
        });
    }
    Ok(probe)
}

/// How the caller proves the drop accounting the seal will carry.
///
/// Under ADR-045 "Drop accounting decision for first slice", zero counts "may be emitted only under
/// one of these explicitly named collection models", and the ADR forbids emitting
/// "an AEE-looking seal with guessed zero drop accounting". This module cannot
/// observe the collection path, so the model is the caller's to declare -- but it is a required
/// argument rather than a default, because a value that can be omitted is a value that gets guessed.
///
/// An earlier version counted the committed observations and refused anything but a lone probe. That
/// measured the wrong thing: two synchronously captured observations are as unbuffered as one, and
/// one observation says nothing about whether others were lost upstream. It also refused every run
/// carrying an interception, which is every run with attack evidence in it.
#[derive(Debug, Clone)]
pub enum DropAccounting {
    /// The sealed observation is the probe result itself, taken synchronously, with no queue between
    /// capture and this builder. Not checkable here; the caller owns the collection path and says so.
    SynchronousProbe,
    /// Every channel between capture and this builder exposes a loss counter, and each was read at
    /// run end. Checkable: a non-zero counter refuses.
    ///
    /// The counter has to come from the producing program. A BPF ring buffer has **no built-in
    /// lost-samples counter**: `BPF_RB_AVAIL_DATA` and the consumer/producer positions are the only
    /// introspection available, and the kernel documentation says they are momentary snapshots "to
    /// be used only for debugging/reporting reasons or for implementing various heuristics". Reading
    /// them as a loss count would carry zero on a channel that lost events. The usable source is a
    /// counter the BPF program increments itself when a reservation fails.
    ///
    /// Completeness is the caller's obligation and is not checkable here: a list that omits the
    /// lossy channel passes. Naming it rather than implying it.
    ///
    /// # The adversary this exists for
    ///
    /// Dropped events are not only an overload symptom, they are an attack. The super producer
    /// threat is a systematic study of exactly this: attackers "may try to compromise the system
    /// auditing frameworks to conceal their malicious activities", and the threat "enables attackers
    /// to either corrupt the auditing framework or paralyze the entire system", with the root cause
    /// identified as "the lack of data isolation in the centralized architecture of existing
    /// solutions" (arXiv 2307.15895, USENIX Security). Follow-on work restates it as provenance
    /// generation being used "to overload a system to force the system to drop security-relevant
    /// events and allow an attacker to hide their actions" (arXiv 2510.08479).
    ///
    /// That sharpens what the omission above costs. A zero count over a **complete** channel list
    /// is evidence that nothing was hidden this way. A zero count over an **incomplete** one is the
    /// attacker's preferred outcome: the seal reports no loss while the unenumerated channel is
    /// where the evidence went. The gap is not a documentation nicety, it is the surface the threat
    /// aims at, so a caller adding a collection path owes an argument for why its list is closed.
    ///
    /// # Why this is recorded rather than solved
    ///
    /// Neither of those papers claims completeness is achievable. 2510.08479 notes that resource
    /// isolation — NODROP's answer — "does not fully solve the problems resulting from hardware
    /// dependencies and performance limitations", and offers a scheduler that "significantly
    /// improves" completeness rather than guaranteeing it. If the state of the art cannot promise a
    /// complete audit trail, an evidence format that *claimed* one would be asserting more than the
    /// field can support.
    ///
    /// So the seal records whether the claim was checked instead of asserting that it holds, which
    /// is what [`DROP_BASIS_CHECKED`] and [`DROP_BASIS_ASSERTED`] carry. A consumer can then price
    /// the difference. That is the honest shape for a property the literature says is approximated,
    /// and it is why the basis travels with the count rather than the count travelling alone.
    CountedQueue { channels: Vec<(String, u64)> },
}

/// Whether the drop-accounting claim was checked here or merely asserted by the caller.
///
/// SLSA v1.2 splits `externalParameters` -- the values passed through the interface -- from
/// `resolvedDependencies`, where the metadata about those values goes. `DropAccounting` is the
/// parameter; whether anything was verified about it is metadata, and a consumer cannot recover it
/// from the model name alone.
///
/// Proof-or-Stop (arXiv 2607.14890) states the rule this encodes: a report that "provides neither
/// verified integrity nor attested execution and carries no binding" is not admissible evidence. A
/// caller-declared model is exactly that, and marking it is the difference between a consumer
/// knowing it and a consumer assuming otherwise. It is the same `observed`/`unknown` split
/// observed-effect v0 already carries on `basis`.
/// # Why `asserted` and not `declared`
///
/// This value was `declared` until the predicate author pointed out a collision on
/// in-toto/attestation#570: his `declared` names the weakest **evidence tier**, ours named a
/// **drop-proof basis**. One word doing two jobs in one thread, and he noted renaming costs less
/// before producer code reads it than after. Measuring it found a third job -- our own
/// `annotation_conformance` records use `declared` for the declared-versus-observed annotation
/// split -- so the word was already overloaded inside this tree.
///
/// `asserted` is not a coined replacement. RFC 9334, the RATS architecture, defines "Claim: A piece
/// of asserted information", and Evidence as "A set of Claims generated by an Attester to be
/// appraised by a Verifier". Asserted is precisely the standardised term for information that has
/// not been appraised, which is exactly what this value reports about a drop count. `checked` keeps
/// its name: it collides with nothing and already says what it means.
pub const DROP_BASIS_CHECKED: &str = "checked";
pub const DROP_BASIS_ASSERTED: &str = "asserted";

impl DropAccounting {
    fn basis(&self) -> &'static str {
        match self {
            Self::SynchronousProbe => DROP_BASIS_ASSERTED,
            Self::CountedQueue { .. } => DROP_BASIS_CHECKED,
        }
    }

    /// The readings the claim rests on, for the variant that has any.
    ///
    /// Checking a counter and then discarding it leaves a consumer told that a check passed and
    /// given nothing to recheck -- the shape this module already rejected once, when a run-phase
    /// challenge was verified inside the producer and never travelled.
    fn channel_readings(&self) -> Vec<String> {
        match self {
            Self::SynchronousProbe => Vec::new(),
            Self::CountedQueue { channels } => channels
                .iter()
                .map(|(name, lost)| format!("{name}={lost}"))
                .collect(),
        }
    }

    fn model(&self) -> &'static str {
        match self {
            Self::SynchronousProbe => DROP_PROOF_SYNCHRONOUS_PROBE,
            Self::CountedQueue { .. } => DROP_PROOF_COUNTED_QUEUE_ZERO,
        }
    }

    fn check(&self) -> Result<(), NotSealEligible> {
        match self {
            Self::SynchronousProbe => Ok(()),
            Self::CountedQueue { channels } => match channels.iter().find(|(_, lost)| *lost != 0) {
                Some((name, lost)) => Err(NotSealEligible::ObservationsLost {
                    channel: name.clone(),
                    lost: *lost,
                }),
                None if channels.is_empty() => Err(NotSealEligible::DropAccountingUnnamed),
                None => Ok(()),
            },
        }
    }
}

/// What a sealed run produces: the seal, and the records its observed set commits to.
///
/// **Not a complete statement.** `observed_set` counts `interception` and `examination` alike, but
/// the checker's row-coverage rule requires at least one `interception` in the refs of a caught
/// `basis: substrate` row. The probe is an examination, so a statement assembled from this output
/// alone yields a valid `aeeObservedSet` *and* a `substrate-row-missing-interception-coverage`
/// rejection. That is not a choice between the two kinds: the examination may sit beside an
/// interception, and the interception has to come from the corpus attack path, which this slice
/// does not build. Named here so the gap does not silently move from the seal to the row.
#[derive(Debug, Clone)]
pub struct SealedRun {
    pub seal: SealPayload,
    pub records: Vec<ObservationRecord>,
}

/// Which vantage observed the run.
///
/// The second collection path (#2093) goes into one substrate under one key, on the predicate
/// author's constraint recorded there: two keys make two substrates and the run binding stops
/// resolving. So the vantage is data in the payload, not an identity in the key.
///
/// That is how SLSA arranges the same problem. Its provenance spec keeps the builder distinct from
/// the signer "in order to support the case where one signer generates attestations for more than
/// one builder", and requires that field "even if it is implicit from the signer, to aid readability
/// and debugging". `assayCollectionPath` is that field here: one observation key covers both paths,
/// and each seal names the vantage that produced it rather than leaving a verifier to infer it from
/// who signed.
pub enum Vantage<'a> {
    /// The kernel path: Landlock TCP-connect, established by an `enforcement_health.v1` record.
    Landlock(&'a EnforcementHealthV1),
    /// The proxy path: the JSON-RPC enforcing proxy, established by its own carriers.
    JsonRpcProxy(&'a ProxyEnforcement),
}

/// A vantage's run-end proof, once its eligibility has been established.
enum RunEndProof<'a> {
    Landlock(&'a Probe),
    Proxy(&'a ProxyDenial),
}

// The key model, and why it is one key rather than per-path keys. Kept as a plain comment block
// because ADR-045 is named here and the quotations below are the ADR's own words; the quotations
// from #2093 and from the SLSA provenance spec live in the doc comment underneath, where they are
// not read as the ADR's. `tests/adr045_citations.rs` enforces that separation.
//
// ADR-045 weighed both arrangements. Its per-path-key option is listed with "stronger evidence-tier
// semantics"; the single-key option carries "weak independence story" and "compromise of one key
// affects all collection paths".
//
// Both readings are right, and they stop competing once the claim is stated. This arrangement does
// not support an independence claim, and #2093 already refuses one on the ground that a single
// operator running both paths cannot support it. A weaker independence story costs nothing that was
// being claimed, and the run binding is worth more than a claim we decline to make.
impl<'a> Vantage<'a> {
    /// The eligibility questions, asked in the vantage's own vocabulary.
    fn check(&self) -> Result<RunEndProof<'a>, NotSealEligible> {
        match self {
            Self::Landlock(h) => seal_eligibility(h).map(RunEndProof::Landlock),
            Self::JsonRpcProxy(e) => proxy_seal_eligibility(e).map(RunEndProof::Proxy),
        }
    }

    /// The schema that established enforcement, reported as `assaySourceSchema`.
    fn source_schema(&self) -> String {
        match self {
            Self::Landlock(h) => h.schema.clone(),
            Self::JsonRpcProxy(_) => PROXY_SOURCE_SCHEMA.to_string(),
        }
    }

    /// What this vantage covers, reported as `assaySealScope`.
    fn scope(&self) -> String {
        match self {
            Self::Landlock(h) => h.scope.clone(),
            Self::JsonRpcProxy(_) => PROXY_SEAL_SCOPE.to_string(),
        }
    }

    /// The non-claims this vantage owes on top of the standing ones.
    ///
    /// The proxy owes one the kernel path does not, and it is the honest difference between them.
    /// Landlock's `restriction_shedding` probe measures that the ruleset could not be shed between
    /// arming and run end; a ruleset outlives the process that installed it. An in-process proxy is
    /// bypassed by not going through it, and no check here can establish otherwise. Recording that
    /// is the difference between two vantages that agree and two vantages a reader can tell apart.
    fn extra_non_claims(&self) -> Vec<String> {
        match self {
            Self::Landlock(_) => Vec::new(),
            Self::JsonRpcProxy(_) => vec![
                "does not prove enforcement could not be bypassed rather than shed".to_string(),
            ],
        }
    }
}

impl RunEndProof<'_> {
    fn examination(&self, rb: &str, collection_path: &str) -> ObservationRecord {
        match self {
            Self::Landlock(p) => probe_examination_record(p, rb, collection_path),
            Self::Proxy(d) => proxy_examination_record(d, rb, collection_path),
        }
    }

    fn label(&self) -> String {
        match self {
            Self::Landlock(p) => observed_label(p),
            Self::Proxy(d) => {
                if d.upstream_reached {
                    "no_call_block".to_string()
                } else {
                    "call_blocked".to_string()
                }
            }
        }
    }
}

/// Assemble the sealed payload for a seal-eligible run, deriving every digest it carries.
///
/// `prior_records` are the interception/examination records already emitted for this run. The probe
/// examination is appended, so the seal commits to a set that includes it.
pub fn build_sealed_run(
    vantage: Vantage<'_>,
    env: &ObservationEnvironment,
    prior_records: &[ObservationRecord],
    sealed_at: &str,
    drop_accounting: &DropAccounting,
    collection_path: &str,
) -> Result<SealedRun, NotSealEligible> {
    let proof = vantage.check()?;
    if !is_rfc3339_utc(sealed_at) {
        return Err(NotSealEligible::UnprovenContextValue { field: "sealed_at" });
    }
    let rb = run_binding(env)?;
    // ADR-045 "Field interpretation" pins `aeePostureDigest` to the carried posture digest and says
    // "It is distinct from the AEE v0.7 run-binding" input taken over the whole object. So this is
    // the digest the posture object *declares*, not the digest *of* that object. They differ by
    // construction -- the declared value is computed before the `digest` member is inserted, so
    // hashing the carried object hashes a strictly larger thing. Two plausible readings, and the ADR
    // requires a negative fixture for the wrong one by name,
    // "`aeePostureDigest` confused with the run-binding digest of the full `networkPosture` object",
    // because it is the one an implementer reaches for.
    let posture_digest = env
        .network_posture
        .get("digest")
        .and_then(|d| d.get("sha256"))
        .and_then(|v| v.as_str())
        .filter(|v| is_sha256_hex(v))
        .ok_or(NotSealEligible::UnprovenContextValue {
            field: "networkPosture.digest.sha256",
        })?
        .to_string();

    let mut records = prior_records.to_vec();
    // The seal's path, not a constant: the examination record and the seal it is sealed into
    // describe the same vantage, so one value flows to both. Passing them separately would let a
    // producer emit a record and a seal that disagree about which path observed the run.
    records.push(proof.examination(&rb, collection_path));

    drop_accounting.check()?;
    let observed = observed_set(&records)?;

    Ok(SealedRun {
        seal: SealPayload {
            aee_kind: "sealed".to_string(),
            aee_version: AEE_VERSION.to_string(),
            aee_run_binding: rb,
            aee_method: "intercepted".to_string(),
            aee_posture_digest: posture_digest,
            aee_still_armed: true,
            aee_drop_count: 0,
            aee_drop_bound: 0,
            aee_observed_set: observed,
            aee_observed_attacks: Vec::new(),
            assay_observed_labels: vec![proof.label()],
            assay_collection_path: collection_path.to_string(),
            assay_sealed_at: sealed_at.to_string(),
            assay_source_schema: vantage.source_schema(),
            assay_seal_scope: vantage.scope(),
            assay_drop_proof_model: drop_accounting.model().to_string(),
            assay_drop_proof_basis: drop_accounting.basis().to_string(),
            assay_drop_channels: drop_accounting.channel_readings(),
            assay_attack_row_attribution_source: "assembly-plane".to_string(),
            assay_non_claims: {
                let mut n = payload_non_claims();
                n.extend(vantage.extra_non_claims());
                n
            },
        },
        records,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enforcement_health_v1::{EnforcementHealthV1, Failure, ReasonCode};

    const PARITY: &str = include_str!(
        "../../../scripts/experiments/fixtures/aee-landlock-seal/derivation-parity.json"
    );

    fn probe(listener_reached: bool, errno: &str) -> Probe {
        Probe {
            kind: "real_block".into(),
            transport: "ipv4".into(),
            blocked_action: "tcp_connect".into(),
            blocked_port: 4444,
            blocked_errno: errno.into(),
            listener_reached,
        }
    }

    fn healthy() -> EnforcementHealthV1 {
        EnforcementHealthV1::landlock_active(
            4,
            vec![443],
            Some(probe(false, "EACCES")),
            Some("restrictions_held".to_string()),
        )
    }

    fn parity() -> serde_json::Value {
        serde_json::from_str(PARITY).expect("parity vectors parse")
    }

    fn env_from_parity() -> ObservationEnvironment {
        let p = parity();
        let e = &p["environment"];
        let g = |k: &str| e[k].as_str().expect("digest").to_string();
        ObservationEnvironment {
            subject_digest: g("subject"),
            substrate_digest: g("substrate"),
            corpus_digest: g("corpus"),
            catch_policy_digest: g("catchPolicy"),
            observation_vocabulary_digest: g("observationVocabulary"),
            run_entropy_digest: g("runEntropy"),
            network_posture: e["networkPosture"].clone(),
        }
    }

    fn refusal(h: &EnforcementHealthV1) -> &'static str {
        build_sealed_run(
            Vantage::Landlock(h),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .expect_err("must refuse")
        .code()
    }

    // ---- derivation parity with the checker -------------------------------------------------

    /// Both sides derive; neither is handed a digest. The vectors are emitted by
    /// `aee_landlock_seal_fixture.py --emit` and read here, so a change on the Python side lands as
    /// a diff in the committed file and a divergence lands as a failure — unlike a transcribed
    /// literal, which stays green while the two implementations drift apart.
    #[test]
    fn run_binding_matches_the_checker() {
        let p = parity();
        assert_eq!(
            run_binding(&env_from_parity()).unwrap(),
            p["expected"]["runBinding"].as_str().unwrap()
        );
    }

    /// The seal must carry the digest the posture object *declares*, not the digest *of* that
    /// object. ADR-045 requires a negative fixture for
    /// "`aeePostureDigest` confused with the run-binding digest of the full `networkPosture` object",
    /// and the two values differ by construction because the declared one is computed before the `digest`
    /// member is inserted. Asserted on the payload rather than on a helper: a test that checks the
    /// function and not where its result lands is how the wrong quantity reached the wire.
    #[test]
    fn the_seal_carries_the_declared_posture_digest_not_the_object_digest() {
        let p = parity();
        let env = env_from_parity();
        let run = build_sealed_run(
            Vantage::Landlock(&healthy()),
            &env,
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .unwrap();
        assert_eq!(
            run.seal.aee_posture_digest,
            p["expected"]["networkPostureDigest"].as_str().unwrap()
        );
        assert_ne!(
            run.seal.aee_posture_digest,
            digest_json(&env.network_posture).unwrap(),
            "the run-binding input is the value the ADR names as the wrong one"
        );
    }

    /// A posture object that declares no usable digest cannot be sealed against.
    #[test]
    fn a_posture_without_a_declared_digest_is_refused() {
        for posture in [
            serde_json::json!({"mode": "deny-default"}),
            serde_json::json!({"mode": "deny-default", "digest": {}}),
            serde_json::json!({"mode": "deny-default", "digest": {"sha256": "NOT-HEX"}}),
        ] {
            let mut env = env_from_parity();
            env.network_posture = posture;
            assert!(
                build_sealed_run(
                    Vantage::Landlock(&healthy()),
                    &env,
                    &[],
                    "2026-08-05T00:00:00Z",
                    &DropAccounting::SynchronousProbe,
                    COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
                )
                .is_err(),
                "a posture that declares no usable digest must not seal"
            );
        }
    }

    #[test]
    fn observed_set_matches_the_checker_over_the_same_records() {
        let p = parity();
        let records: Vec<ObservationRecord> = p["records"]
            .as_array()
            .unwrap()
            .iter()
            .map(|r| ObservationRecord {
                payload: r["payload"].clone(),
                payload_type: r["payloadType"].as_str().unwrap().to_string(),
            })
            .collect();
        assert_eq!(
            observed_set(&records).unwrap(),
            p["expected"]["observedSet"].as_str().unwrap()
        );
    }

    /// The sort is only observable with more than one leaf, and the single-leaf vector above cannot
    /// see it: an implementation that omits `sort` matches on one element. AEE v0.7 specifies the
    /// observed set as leaf hashes "sorted ascending by UTF-16 code unit", so a producer emitting in
    /// arrival order matches on one observation and diverges on two -- silently, and only once a
    /// real run has more than one.
    #[test]
    fn the_leaf_sort_is_observable_and_matches_the_checker() {
        let p = parity();
        let records: Vec<ObservationRecord> = p["orderingRecords"]
            .as_array()
            .expect("ordering vector present")
            .iter()
            .map(|r| ObservationRecord {
                payload: r["payload"].clone(),
                payload_type: r["payloadType"].as_str().unwrap().to_string(),
            })
            .collect();
        let leaves: Vec<String> = records.iter().map(|r| leaf_hash(r).unwrap()).collect();
        let mut sorted = leaves.clone();
        sorted.sort_unstable();
        assert_ne!(
            leaves, sorted,
            "the vector must emit out of sorted order, or this test proves nothing"
        );
        assert_eq!(
            observed_set(&records).unwrap(),
            p["expected"]["orderingObservedSet"].as_str().unwrap()
        );
    }

    // ---- what the commitment does and does not carry ------------------------------------------

    /// The probe leaf is inside the commitment, so a consumer recomputing the set sees the same
    /// observation the seal was built from. That is all it shows: the leaf is a value the producer
    /// chose, so this says nothing about when the probe ran.
    #[test]
    fn the_seal_commits_to_the_probe_examination_leaf() {
        let run = build_sealed_run(
            Vantage::Landlock(&healthy()),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .unwrap();
        let probe_rec = run
            .records
            .iter()
            .find(|r| r.payload["aeeKind"] == "examination")
            .expect("the probe is emitted as an examination record");
        assert_eq!(
            run.seal.aee_observed_set,
            observed_set(std::slice::from_ref(probe_rec)).unwrap()
        );
        assert_ne!(
            run.seal.aee_observed_set,
            observed_set(&[]).unwrap(),
            "an empty set is not a commitment to anything"
        );
    }

    /// A run carrying attack evidence must still be sealable. The previous gate counted committed
    /// observations and refused anything but a lone probe, which forbade exactly the statements
    /// ADR-045 allows -- "zero or more interception records per run" -- and the checker
    /// requires for a caught substrate row.
    #[test]
    fn a_run_carrying_an_interception_can_still_seal() {
        let prior = vec![ObservationRecord {
            payload: serde_json::json!({"aeeKind": "interception", "aeeVersion": "0.7", "x": 1}),
            payload_type: OBSERVATION_PAYLOAD_TYPE.to_string(),
        }];
        let run = build_sealed_run(
            Vantage::Landlock(&healthy()),
            &env_from_parity(),
            &prior,
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .expect("an interception must not block the seal");
        assert_eq!(run.records.len(), 2);
        assert_ne!(
            run.seal.aee_observed_set,
            observed_set(&run.records[1..]).unwrap(),
            "dropping the interception must move the commitment"
        );
    }

    /// The counted-queue model is the one this module can check, and it refuses on a lost
    /// observation rather than carrying zero beside it.
    #[test]
    fn a_counted_queue_that_lost_an_observation_is_refused() {
        let lossy = DropAccounting::CountedQueue {
            // One lost observation is the boundary; a test using 3 cannot see `lost > 1`.
            channels: vec![("probe-ring".into(), 0), ("event-ring".into(), 1)],
        };
        let err = build_sealed_run(
            Vantage::Landlock(&healthy()),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &lossy,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .expect_err("a lost observation cannot carry zero");
        assert_eq!(err.code(), "observations-lost");

        let empty = DropAccounting::CountedQueue { channels: vec![] };
        assert_eq!(
            build_sealed_run(
                Vantage::Landlock(&healthy()),
                &env_from_parity(),
                &[],
                "2026-08-05T00:00:00Z",
                &empty,
                COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
            )
            .expect_err("no channels proves nothing")
            .code(),
            "drop-accounting-unnamed"
        );

        let clean = DropAccounting::CountedQueue {
            channels: vec![("probe-ring".into(), 0)],
        };
        let run = build_sealed_run(
            Vantage::Landlock(&healthy()),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &clean,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .expect("all counters zero");
        assert_eq!(
            run.seal.assay_drop_proof_model, DROP_PROOF_COUNTED_QUEUE_ZERO,
            "the payload names the model the caller proved, not a constant"
        );
        assert_eq!(run.seal.assay_drop_proof_basis, DROP_BASIS_CHECKED);
        // The readings travel. Checking a counter and discarding it leaves a consumer told that a
        // check passed with nothing to recheck -- the defect this module already fixed once.
        assert_eq!(
            run.seal.assay_drop_channels,
            vec!["probe-ring=0".to_string()]
        );
    }

    /// Every value the seal carries, asserted where it lands.
    #[test]
    fn every_carried_value_lands_in_its_own_payload_field() {
        let p = parity();
        let h = healthy();
        let run = build_sealed_run(
            Vantage::Landlock(&h),
            &env_from_parity(),
            &[],
            "2026-08-05T12:34:56Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .unwrap();
        let s = &run.seal;
        assert_eq!(s.aee_kind, "sealed");
        assert_eq!(s.aee_version, "0.7");
        assert_eq!(s.aee_method, "intercepted");
        assert!(s.aee_still_armed);
        assert_eq!((s.aee_drop_count, s.aee_drop_bound), (0, 0));
        assert_eq!(
            s.aee_run_binding,
            p["expected"]["runBinding"].as_str().unwrap()
        );
        assert_eq!(
            s.aee_posture_digest,
            p["expected"]["networkPostureDigest"].as_str().unwrap()
        );
        assert_eq!(s.aee_observed_set, observed_set(&run.records).unwrap());
        assert!(s.aee_observed_attacks.is_empty());
        assert_eq!(
            s.assay_collection_path,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT
        );
        assert_eq!(s.assay_sealed_at, "2026-08-05T12:34:56Z");
        assert_eq!(s.assay_source_schema, h.schema);
        assert_eq!(s.assay_seal_scope, h.scope);
        assert_eq!(s.assay_drop_proof_model, DROP_PROOF_SYNCHRONOUS_PROBE);
        assert_eq!(
            s.assay_drop_proof_basis, DROP_BASIS_ASSERTED,
            "a caller declaration is not a check, and the payload must say which it was"
        );
        assert!(
            s.assay_drop_channels.is_empty(),
            "nothing was read, so nothing is carried"
        );
        assert_eq!(s.assay_attack_row_attribution_source, "assembly-plane");
        assert_eq!(s.assay_observed_labels, vec!["connect_blocked".to_string()]);
        // Exact, not subset: `.any()` licenses any addition to a signed field, and ADR-042's stop
        // list bans exactly the kind of claim someone would append.
        assert_eq!(
            s.assay_non_claims,
            vec![
                "does not prove complete run population".to_string(),
                "does not prove agent safety".to_string(),
                "does not prove provider side effects".to_string(),
                "does not prove independent substrate operation".to_string(),
                "does not distinguish withdrawn coverage from coverage never held".to_string(),
            ]
        );
    }

    /// The wire names the checker reads. A count cannot see a rename.
    #[test]
    fn the_payload_member_names_are_the_ones_the_checker_reads() {
        let run = build_sealed_run(
            Vantage::Landlock(&healthy()),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .unwrap();
        let value = serde_json::to_value(&run.seal).unwrap();
        let mut got: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        got.sort_unstable();
        assert_eq!(
            got,
            [
                "aeeDropBound",
                "aeeDropCount",
                "aeeKind",
                "aeeMethod",
                "aeeObservedAttacks",
                "aeeObservedSet",
                "aeePostureDigest",
                "aeeRunBinding",
                "aeeStillArmed",
                "aeeVersion",
                "assayAttackRowAttributionSource",
                "assayCollectionPath",
                "assayDropChannels",
                "assayDropProofBasis",
                "assayDropProofModel",
                "assayNonClaims",
                "assayObservedLabels",
                "assaySealScope",
                "assaySealedAt",
                "assaySourceSchema",
            ]
        );
    }

    /// The examination record is the observation, and its members travel inside the leaf the seal
    /// commits to. `assayCollectionPath` is the one that bites: the checker reads it on every record
    /// for the key-scope check.
    #[test]
    fn the_examination_record_carries_the_probe_it_was_built_from() {
        let h = healthy();
        let probe = h.probe.as_ref().unwrap().clone();
        let run = build_sealed_run(
            Vantage::Landlock(&h),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .unwrap();
        let e = run
            .records
            .iter()
            .find(|r| r.payload["aeeKind"] == "examination")
            .unwrap();
        // A literal, not the constant: the field is filled *from* that constant, so comparing the
        // two compares it to itself. It is an argument to `build_pae`, so a change to it silently
        // rewrites every leaf hash and every observed set.
        assert_eq!(
            e.payload_type,
            "application/vnd.assay.aee-landlock-seal.fixture.v0+json"
        );
        assert_eq!(e.payload["aeeRunBinding"], run.seal.aee_run_binding);
        assert_eq!(e.payload["aeeVersion"], "0.7");
        assert_eq!(e.payload["aeeMethod"], "intercepted");
        assert_eq!(
            e.payload["assayCollectionPath"],
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT
        );
        assert_eq!(e.payload["assayProbeTransport"], probe.transport);
        assert_eq!(e.payload["assayProbeAction"], probe.blocked_action);
        assert_eq!(e.payload["assayProbePort"], probe.blocked_port);
        assert_eq!(e.payload["assayProbeErrno"], probe.blocked_errno);
        assert_eq!(
            e.payload["assayProbeListenerReached"],
            probe.listener_reached
        );
    }

    /// Calendar-invalid instants the shape check accepted and the checker's `strptime` refuses.
    /// Each is 20 chars ending in Z, so none dies on the length check.
    #[test]
    fn a_calendar_invalid_instant_is_refused() {
        for bad in [
            "2026-02-30T00:00:00Z",
            "9999-99-99T99:99:99Z",
            "2026-08-05T24:00:00Z",
            "2027-02-29T00:00:00Z",
            "2026-00-05T00:00:00Z",
            "2026-08-00T00:00:00Z",
            // Gregorian century rule: 2100 is not a leap year. `year % 4 == 0` alone accepts it.
            "2100-02-29T00:00:00Z",
            // `strptime` has no year 0, and an unparsable instant makes the checker skip its whole
            // key-validity-window loop.
            "0000-01-01T00:00:00Z",
            // A leap second: `datetime` rejects second=60, so both sides must.
            "2026-06-30T23:59:60Z",
        ] {
            assert!(
                build_sealed_run(
                    Vantage::Landlock(&healthy()),
                    &env_from_parity(),
                    &[],
                    bad,
                    &DropAccounting::SynchronousProbe,
                    COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
                )
                .is_err(),
                "sealed_at {bad:?} must be refused"
            );
        }
        for good in ["2028-02-29T23:59:59Z", "2000-02-29T23:59:59Z"] {
            assert!(
                build_sealed_run(
                    Vantage::Landlock(&healthy()),
                    &env_from_parity(),
                    &[],
                    good,
                    &DropAccounting::SynchronousProbe,
                    COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
                )
                .is_ok(),
                "{good:?} is a real leap day and must still seal"
            );
        }
    }

    // ---- refusals ----------------------------------------------------------------------------

    #[test]
    fn a_run_without_a_run_end_probe_is_refused() {
        let h = EnforcementHealthV1::landlock_active(
            4,
            vec![443],
            None,
            Some("restrictions_held".to_string()),
        );
        assert!(
            h.landlock.restrict_self_confirmed,
            "the start-time fact is present"
        );
        assert_eq!(refusal(&h), "no-run-end-probe");
    }

    #[test]
    fn an_abi_that_cannot_express_the_restriction_is_refused() {
        for abi in [1, 2, 3] {
            let h = EnforcementHealthV1::landlock_active(
                abi,
                vec![443],
                Some(probe(false, "EACCES")),
                Some("restrictions_held".to_string()),
            );
            assert_eq!(refusal(&h), "abi-cannot-express-restriction", "abi {abi}");
        }
    }

    #[test]
    fn weak_and_absent_block_signals_are_refused() {
        for errno in ["ECONNREFUSED", "ETIMEDOUT", "", "eacces", "EACCES "] {
            let h = EnforcementHealthV1::landlock_active(
                4,
                vec![443],
                Some(probe(false, errno)),
                Some("restrictions_held".to_string()),
            );
            assert_eq!(refusal(&h), "probe-signal-too-weak", "errno {errno:?}");
        }
        let h = EnforcementHealthV1::landlock_active(
            4,
            vec![443],
            Some(probe(true, "EACCES")),
            Some("restrictions_held".to_string()),
        );
        assert_eq!(refusal(&h), "probe-reached-listener");
    }

    #[test]
    fn a_record_that_contradicts_itself_is_refused() {
        let mut h = healthy();
        h.failure = Some(Failure {
            reason_code: ReasonCode::RestrictSelfFailed,
            detail: "x".into(),
        });
        assert_eq!(refusal(&h), "record-self-contradictory");

        let mut h = healthy();
        h.landlock.restrict_self_confirmed = false;
        assert_eq!(refusal(&h), "record-self-contradictory");

        let mut h = healthy();
        h.landlock.net_connect_tcp_supported = Some(false);
        assert_eq!(refusal(&h), "record-self-contradictory");
    }

    #[test]
    fn a_seal_instant_that_is_not_an_instant_is_refused() {
        for bad in [
            "yesterday afternoon",
            "",
            "2026-08-05",
            "2026-08-05T00:00:00",
        ] {
            assert!(
                build_sealed_run(
                    Vantage::Landlock(&healthy()),
                    &env_from_parity(),
                    &[],
                    bad,
                    &DropAccounting::SynchronousProbe,
                    COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
                )
                .is_err(),
                "sealed_at {bad:?} must be refused"
            );
        }
    }

    // ---- derived, not asserted ----------------------------------------------------------------

    #[test]
    fn derived_fields_come_from_the_record_not_from_constants() {
        let mut h = healthy();
        h.schema = "assay.enforcement_health.v0".into();
        let run = build_sealed_run(
            Vantage::Landlock(&h),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .unwrap();
        assert_eq!(
            run.seal.assay_source_schema, "assay.enforcement_health.v0",
            "must not relabel the artifact"
        );
        assert_eq!(
            run.seal.assay_seal_scope, h.scope,
            "the seal scope is the record's, not a constant"
        );
    }

    /// A record from before the shedding measurement existed does not seal.
    ///
    /// This is the whole point of the gate and it is deliberately a *fixture*, not a hand-built
    /// struct: every enforcement-health record written before this field existed looks exactly like
    /// this, and each one would otherwise have sealed with `aeeStillArmed: true` resting on an
    /// invariant CVE-2024-42318 shows has an exception.
    #[test]
    fn a_record_that_never_measured_shedding_does_not_seal() {
        let raw = include_str!(
            "../tests/fixtures/enforcement_health/v1/active_probe_shedding_unmeasured.json"
        );
        let h: EnforcementHealthV1 = serde_json::from_str(raw).expect("fixture parses");
        let err = build_sealed_run(
            Vantage::Landlock(&h),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .expect_err("a record with no measurement must not seal");
        assert_eq!(err.code(), "restriction-shedding-not-established");
        assert!(err.to_string().contains("not measured"), "{err}");
    }

    /// The two non-held outcomes are refused for their own reasons, and `inconclusive` is the one
    /// that matters: a probe that could not run has found nothing, which is not a finding that
    /// restrictions hold. Treating it as one is the fail-open the three-valued outcome exists for.
    #[test]
    fn a_shed_or_inconclusive_measurement_does_not_seal() {
        for value in ["restrictions_shed", "inconclusive"] {
            let mut h = healthy();
            h.landlock.restriction_shedding = Some(value.to_string());
            let err = build_sealed_run(
                Vantage::Landlock(&h),
                &env_from_parity(),
                &[],
                "2026-08-05T00:00:00Z",
                &DropAccounting::SynchronousProbe,
                COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
            )
            .expect_err("must not seal");
            assert_eq!(err.code(), "restriction-shedding-not-established");
            assert!(err.to_string().contains(value), "{err}");
        }
    }

    /// The committed June carrier fixture is an applied-ruleset shape, and it seals. That is the
    /// honest limit of this module: it is seal-eligible on every field it reads, and nothing here
    /// establishes when the probe ran. The observed-set commitment carries the observation to a
    /// consumer; it does not make the probe's position in the run checkable.
    #[test]
    fn the_committed_carrier_fixture_still_parses_and_seals_through_the_commitment() {
        let raw = include_str!("../tests/fixtures/enforcement_health/v1/active_with_probe.json");
        let h: EnforcementHealthV1 = serde_json::from_str(raw).expect("fixture parses");
        let run = build_sealed_run(
            Vantage::Landlock(&h),
            &env_from_parity(),
            &[],
            "2026-08-05T00:00:00Z",
            &DropAccounting::SynchronousProbe,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
        )
        .expect("eligible");
        assert_eq!(
            run.records.len(),
            1,
            "the probe is the only committed observation"
        );
        assert_eq!(
            run.seal.aee_observed_set,
            observed_set(&run.records).unwrap()
        );
    }
}
