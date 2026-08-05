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
pub const COLLECTION_PATH_LANDLOCK_TCP_CONNECT: &str = "landlock-tcp-connect";
/// AEE draft version this payload is shape-compatible with. Not a conformance claim.
pub const AEE_VERSION: &str = "0.7";
/// The only drop-accounting proof model the Landlock-first slice can honestly carry.
pub const DROP_PROOF_SYNCHRONOUS_PROBE: &str = "synchronous-probe";

/// Landlock denies `connect(2)` with `EACCES`. ADR-045 and `enforcement_health.v1` agree that weak
/// signals never count as a block: a timeout or `ECONNREFUSED` can be produced by a host that is
/// simply not listening, which is indistinguishable from enforcement that was never armed.
const BLOCKING_ERRNO: &str = "EACCES";

/// `LANDLOCK_ACCESS_NET_CONNECT_TCP` was introduced in Landlock ABI 4 (Linux 6.7). A kernel below
/// that cannot express a TCP-connect restriction at all.
const LANDLOCK_ABI_NET_CONNECT_TCP: u32 = 4;

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
    /// The probe carries no run-phase challenge, so it is indistinguishable from one taken at
    /// arming time. RFC 9334 freshness: without a challenge there is nothing to narrow recentness.
    ProbePhaseUnproven,
    /// The probe's challenge is not the one this run issued after corpus injection.
    ProbeChallengeMismatch,
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
            Self::ProbePhaseUnproven => "probe-phase-unproven",
            Self::ProbeChallengeMismatch => "probe-challenge-mismatch",
            Self::AbiCannotExpressRestriction { .. } => "abi-cannot-express-restriction",
            Self::RecordSelfContradictory { .. } => "record-self-contradictory",
            Self::UnprovenContextValue { .. } => "unproven-context-value",
            Self::DerivationFailed { .. } => "derivation-failed",
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
            Self::ProbePhaseUnproven => write!(f, "the probe carries no run-phase challenge, so it is indistinguishable from one taken at arming time"),
            Self::ProbeChallengeMismatch => write!(f, "the probe's challenge is not the one this run issued"),
            Self::AbiCannotExpressRestriction { abi } => write!(f, "Landlock ABI {abi} predates LANDLOCK_ACCESS_NET_CONNECT_TCP (ABI 4), so this denial did not come from Landlock"),
            Self::RecordSelfContradictory { detail } => write!(f, "health record contradicts itself: {detail}"),
            Self::UnprovenContextValue { field } => write!(f, "run-context field {field} is not a value this run proved"),
            Self::DerivationFailed { what } => write!(f, "could not derive a required digest: {what}"),
        }
    }
}

/// Lowercase SHA-256 hex, the shape the #2001 field contract requires of every digest member.
fn is_sha256_hex(v: &str) -> bool {
    v.len() == 64
        && v.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

/// `YYYY-MM-DDTHH:MM:SSZ`, the shape the ADR-045 checker's validity-window check parses.
fn is_rfc3339_utc(v: &str) -> bool {
    let b = v.as_bytes();
    b.len() == 20
        && b[4] == b'-'
        && b[7] == b'-'
        && b[10] == b'T'
        && b[13] == b':'
        && b[16] == b':'
        && b[19] == b'Z'
        && [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18]
            .iter()
            .all(|&i| b[i].is_ascii_digit())
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
    #[serde(rename = "assayAttackRowAttributionSource")]
    pub assay_attack_row_attribution_source: String,
    #[serde(rename = "assayNonClaims")]
    pub assay_non_claims: Vec<String>,
}

/// The payload-local minimum non-claims from the #2001 payload contract.
fn payload_non_claims() -> Vec<String> {
    [
        "does not prove complete run population",
        "does not prove agent safety",
        "does not prove provider side effects",
        "does not prove independent substrate operation",
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
#[derive(Debug, Clone)]
pub struct ObservationRecord {
    pub payload: serde_json::Value,
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

/// AEE v0.7 `aeeObservedSet`: a digest over the sorted lowercase leaf hashes of every emitted
/// `interception` and `examination` record.
///
/// A commitment by a party that does not control the carried set: dropping a record removes a leaf
/// and the value diverges. Deriving it here rather than accepting one is what makes the seal's
/// position in the run checkable — a seal committing to a leaf could not have been computed before
/// that leaf existed.
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
/// The probe becomes a record rather than staying a field in a health artifact, and that is the
/// whole point: the seal's `aeeObservedSet` then commits to its leaf, so the seal demonstrably was
/// computed after the probe. A field in a side artifact never travels and proves nothing to a
/// consumer.
///
/// It also makes the drop-accounting model readable instead of asserted. Under the synchronous-probe
/// model the only sealed observation *is* the probe result; with the probe as the sole member of the
/// observed set, that is a property of the statement rather than a label the producer chose.
pub fn probe_examination_record(probe: &Probe, run_binding: &str) -> ObservationRecord {
    ObservationRecord {
        payload: serde_json::json!({
            "aeeKind": "examination",
            "aeeVersion": AEE_VERSION,
            "aeeRunBinding": run_binding,
            "aeeMethod": "intercepted",
            "assayCollectionPath": COLLECTION_PATH_LANDLOCK_TCP_CONNECT,
            "assayProbeTransport": probe.transport,
            "assayProbeAction": probe.blocked_action,
            "assayProbePort": probe.blocked_port,
            "assayProbeErrno": probe.blocked_errno,
            "assayProbeListenerReached": probe.listener_reached,
        }),
        payload_type: OBSERVATION_PAYLOAD_TYPE.to_string(),
    }
}

pub fn seal_eligibility(health: &EnforcementHealthV1) -> Result<&Probe, NotSealEligible> {
    if health.status != Status::Active {
        return Err(NotSealEligible::NotArmed {
            status: health.status,
        });
    }
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
    if health.failure.is_some() {
        return Err(NotSealEligible::RecordSelfContradictory {
            detail: "status is active and a failure is recorded".into(),
        });
    }
    if !health.landlock.no_new_privs_confirmed || !health.landlock.restrict_self_confirmed {
        return Err(NotSealEligible::RecordSelfContradictory {
            detail: "status is active but the ruleset was never confirmed applied".into(),
        });
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

/// What a sealed run produces: the seal, and the records its observed set commits to.
#[derive(Debug, Clone)]
pub struct SealedRun {
    pub seal: SealPayload,
    pub records: Vec<ObservationRecord>,
}

/// Assemble the sealed payload for a seal-eligible run, deriving every digest it carries.
///
/// `prior_records` are the interception/examination records already emitted for this run. The probe
/// examination is appended, so the seal commits to a set that includes it.
pub fn build_sealed_run(
    health: &EnforcementHealthV1,
    env: &ObservationEnvironment,
    prior_records: &[ObservationRecord],
    sealed_at: &str,
) -> Result<SealedRun, NotSealEligible> {
    let probe = seal_eligibility(health)?;
    if !is_rfc3339_utc(sealed_at) {
        return Err(NotSealEligible::UnprovenContextValue { field: "sealed_at" });
    }
    let rb = run_binding(env)?;
    // ADR-045 line 192, and its line 476 negative fixture: `aeePostureDigest` is the digest the
    // posture object *declares*, not the digest *of* that object. They differ by construction --
    // the declared value is computed before the `digest` member is inserted, so hashing the carried
    // object hashes a strictly larger thing. Two plausible readings, and the ADR names the wrong one
    // by name because it is the one an implementer reaches for.
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
    records.push(probe_examination_record(probe, &rb));
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
            assay_observed_labels: vec![observed_label(probe)],
            assay_collection_path: COLLECTION_PATH_LANDLOCK_TCP_CONNECT.to_string(),
            assay_sealed_at: sealed_at.to_string(),
            assay_source_schema: health.schema.clone(),
            assay_seal_scope: health.scope.clone(),
            assay_drop_proof_model: DROP_PROOF_SYNCHRONOUS_PROBE.to_string(),
            assay_attack_row_attribution_source: "assembly-plane".to_string(),
            assay_non_claims: payload_non_claims(),
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
        EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(false, "EACCES")))
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
        build_sealed_run(h, &env_from_parity(), &[], "2026-08-05T00:00:00Z")
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
    /// object. ADR-045 line 476 names this confusion as a required negative fixture, and the two
    /// values differ by construction because the declared one is computed before the `digest`
    /// member is inserted. Asserted on the payload rather than on a helper: a test that checks the
    /// function and not where its result lands is how the wrong quantity reached the wire.
    #[test]
    fn the_seal_carries_the_declared_posture_digest_not_the_object_digest() {
        let p = parity();
        let env = env_from_parity();
        let run = build_sealed_run(&healthy(), &env, &[], "2026-08-05T00:00:00Z").unwrap();
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
                build_sealed_run(&healthy(), &env, &[], "2026-08-05T00:00:00Z").is_err(),
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

    // ---- the phase property, now carried rather than asserted ---------------------------------

    /// The reason the probe becomes a record. The seal's observed set commits to the probe leaf, so
    /// a consumer recomputing it learns the seal was built after the probe. Nothing here rests on
    /// the producer's word.
    #[test]
    fn the_seal_commits_to_the_probe_examination_leaf() {
        let run =
            build_sealed_run(&healthy(), &env_from_parity(), &[], "2026-08-05T00:00:00Z").unwrap();
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

    /// The drop-accounting model becomes readable instead of asserted: under synchronous-probe the
    /// only sealed observation *is* the probe result, and with the probe as the sole member that is
    /// a property of the statement rather than a label the producer chose.
    #[test]
    fn the_synchronous_probe_model_is_visible_in_the_committed_set() {
        let run =
            build_sealed_run(&healthy(), &env_from_parity(), &[], "2026-08-05T00:00:00Z").unwrap();
        let committed: Vec<_> = run
            .records
            .iter()
            .filter(|r| {
                matches!(
                    r.payload["aeeKind"].as_str(),
                    Some("interception") | Some("examination")
                )
            })
            .collect();
        assert_eq!(
            committed.len(),
            1,
            "synchronous-probe: exactly one sealed observation"
        );
        assert_eq!(committed[0].payload["aeeKind"], "examination");
        assert_eq!(
            run.seal.assay_drop_proof_model,
            DROP_PROOF_SYNCHRONOUS_PROBE
        );
    }

    /// With prior interceptions the set grows, so the seal commits to those too and the
    /// sole-member reading no longer holds — which is exactly when the model must not be claimed.
    #[test]
    fn prior_records_enter_the_commitment() {
        let prior = vec![ObservationRecord {
            payload: serde_json::json!({"aeeKind": "interception", "aeeVersion": "0.7", "x": 1}),
            payload_type: OBSERVATION_PAYLOAD_TYPE.to_string(),
        }];
        let a =
            build_sealed_run(&healthy(), &env_from_parity(), &[], "2026-08-05T00:00:00Z").unwrap();
        let b = build_sealed_run(
            &healthy(),
            &env_from_parity(),
            &prior,
            "2026-08-05T00:00:00Z",
        )
        .unwrap();
        assert_ne!(
            a.seal.aee_observed_set, b.seal.aee_observed_set,
            "a dropped record must move the value"
        );
    }

    /// Every value the seal carries, asserted where it lands. The previous rewrite dropped this
    /// module and fourteen of fifteen payload mutants survived -- `aeeKind: "arming"`,
    /// `aeeStillArmed: false`, an empty run binding, emptied non-claims. Checking a derivation
    /// function proves nothing about the field it is supposed to fill.
    #[test]
    fn every_carried_value_lands_in_its_own_payload_field() {
        let p = parity();
        let env = env_from_parity();
        let h = healthy();
        let run = build_sealed_run(&h, &env, &[], "2026-08-05T12:34:56Z").unwrap();
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
        assert_eq!(s.assay_attack_row_attribution_source, "assembly-plane");
        assert_eq!(s.assay_observed_labels, vec!["connect_blocked".to_string()]);
        for required in [
            "does not prove complete run population",
            "does not prove agent safety",
            "does not prove provider side effects",
            "does not prove independent substrate operation",
        ] {
            assert!(
                s.assay_non_claims.iter().any(|c| c == required),
                "missing non-claim: {required}"
            );
        }

        // The examination record is the observation. Emptying it must not go unnoticed.
        let exam = run
            .records
            .iter()
            .find(|r| r.payload["aeeKind"] == "examination")
            .unwrap();
        assert_eq!(exam.payload["assayProbeErrno"], "EACCES");
        assert_eq!(exam.payload["assayProbeListenerReached"], false);
        assert_eq!(exam.payload["aeeRunBinding"], s.aee_run_binding);
    }

    /// The member set the checker reads, pinned where it is emitted.
    #[test]
    fn the_payload_member_set_is_the_eighteen_the_checker_carries() {
        let run =
            build_sealed_run(&healthy(), &env_from_parity(), &[], "2026-08-05T00:00:00Z").unwrap();
        let value = serde_json::to_value(&run.seal).unwrap();
        let mut got: Vec<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        got.sort_unstable();
        assert_eq!(got.len(), 18, "members: {got:?}");
    }

    // ---- refusals ----------------------------------------------------------------------------

    #[test]
    fn a_run_without_a_run_end_probe_is_refused() {
        let h = EnforcementHealthV1::landlock_active(4, vec![443], None);
        assert!(
            h.landlock.restrict_self_confirmed,
            "the start-time fact is present"
        );
        assert_eq!(refusal(&h), "no-run-end-probe");
    }

    #[test]
    fn an_abi_that_cannot_express_the_restriction_is_refused() {
        for abi in [1, 2, 3] {
            let h =
                EnforcementHealthV1::landlock_active(abi, vec![443], Some(probe(false, "EACCES")));
            assert_eq!(refusal(&h), "abi-cannot-express-restriction", "abi {abi}");
        }
    }

    #[test]
    fn weak_and_absent_block_signals_are_refused() {
        for errno in ["ECONNREFUSED", "ETIMEDOUT", "", "eacces", "EACCES "] {
            let h = EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(false, errno)));
            assert_eq!(refusal(&h), "probe-signal-too-weak", "errno {errno:?}");
        }
        let h = EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(true, "EACCES")));
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
                build_sealed_run(&healthy(), &env_from_parity(), &[], bad).is_err(),
                "sealed_at {bad:?} must be refused"
            );
        }
    }

    // ---- derived, not asserted ----------------------------------------------------------------

    #[test]
    fn derived_fields_come_from_the_record_not_from_constants() {
        let mut h = healthy();
        h.schema = "assay.enforcement_health.v0".into();
        let run = build_sealed_run(&h, &env_from_parity(), &[], "2026-08-05T00:00:00Z").unwrap();
        assert_eq!(
            run.seal.assay_source_schema, "assay.enforcement_health.v0",
            "must not relabel the artifact"
        );
        assert_eq!(
            run.seal.assay_seal_scope, h.scope,
            "the seal scope is the record's, not a constant"
        );
    }

    /// The committed June carrier fixture is an applied-ruleset shape. It is seal-eligible on every
    /// field this module reads, which is the honest limit: what makes its seal checkable is the
    /// observed-set commitment, not a field in the health record.
    #[test]
    fn the_committed_carrier_fixture_still_parses_and_seals_through_the_commitment() {
        let raw = include_str!("../tests/fixtures/enforcement_health/v1/active_with_probe.json");
        let h: EnforcementHealthV1 = serde_json::from_str(raw).expect("fixture parses");
        let run = build_sealed_run(&h, &env_from_parity(), &[], "2026-08-05T00:00:00Z")
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
