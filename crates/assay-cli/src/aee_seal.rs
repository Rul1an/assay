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
use serde::{Deserialize, Serialize};

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
        }
    }
}

/// Run-level inputs the seal binds to, supplied by the caller because none of them are derivable
/// from an enforcement-health record alone.
#[derive(Debug, Clone)]
pub struct RunContext {
    /// AEE run binding over the run's pre-injection inputs.
    pub run_binding: String,
    /// MUST equal the carried `observationEnvironment.networkPosture.digest.sha256`.
    pub posture_digest: String,
    /// AEE digest commitment over emitted `interception`/`examination` record leaves.
    pub observed_set: String,
    /// The sealed enforcement scope, e.g. `tcp_connect_landlock_port`.
    pub seal_scope: String,
    /// RFC 3339 UTC instant the seal commits to, checked against the signing key's validity window.
    pub sealed_at: String,
}

/// The ADR-045 sealed payload, assembled but **not signed**.
///
/// Field names are the on-wire ones so this type is the single place the shape is stated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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
    /// Empty for a pure Landlock path: the signer did not dispatch the corpus attack, so it cannot
    /// bind an attack id to its own observation. ADR-045 calls this the safe lower bound.
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

/// Decide whether this run-end health record can carry a seal, and say why not when it cannot.
///
/// Only the conditions decidable from the health record are evaluated here. Run binding, posture
/// digest and observed set are supplied through [`RunContext`]; whether *they* are derivable is the
/// caller's precondition, not something this function can check.
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
    // The still-armed rule, and the whole reason this function is not a boolean on the health
    // record: `restrict_self_confirmed` is set at arming time and stays set in the record whatever
    // happened afterwards, so reading it as run-end state would turn a start-time fact into an
    // end-time claim.
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

/// Assemble the sealed payload for a seal-eligible run.
///
/// Returns the same refusal as [`seal_eligibility`] when the run is not eligible, so there is no
/// path from an ineligible run to a payload. Drop accounting is emitted as zero under the
/// synchronous-probe model only: the sealed observation *is* the probe result, taken synchronously,
/// with no queue between capture and this builder that could lose an observation unnoticed.
pub fn build_seal_payload(
    health: &EnforcementHealthV1,
    ctx: &RunContext,
) -> Result<SealPayload, NotSealEligible> {
    let _probe = seal_eligibility(health)?;
    Ok(SealPayload {
        aee_kind: "sealed".to_string(),
        aee_version: AEE_VERSION.to_string(),
        aee_run_binding: ctx.run_binding.clone(),
        aee_method: "intercepted".to_string(),
        aee_posture_digest: ctx.posture_digest.clone(),
        aee_still_armed: true,
        aee_drop_count: 0,
        aee_drop_bound: 0,
        aee_observed_set: ctx.observed_set.clone(),
        aee_observed_attacks: Vec::new(),
        assay_observed_labels: vec!["connect_blocked".to_string()],
        assay_collection_path: COLLECTION_PATH_LANDLOCK_TCP_CONNECT.to_string(),
        assay_sealed_at: ctx.sealed_at.clone(),
        assay_source_schema: crate::enforcement_health_v1::SCHEMA_V1.to_string(),
        assay_seal_scope: ctx.seal_scope.clone(),
        assay_drop_proof_model: DROP_PROOF_SYNCHRONOUS_PROBE.to_string(),
        // Assembly-plane, not substrate-runner: this signer observed a blocked connect, it did not
        // dispatch the corpus attack, so it cannot attribute an attack id to its own observation.
        assay_attack_row_attribution_source: "assembly-plane".to_string(),
        assay_non_claims: payload_non_claims(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::enforcement_health_v1::EnforcementHealthV1;

    fn probe(listener_reached: bool, errno: &str) -> Probe {
        Probe {
            kind: "real_block".to_string(),
            transport: "tcp".to_string(),
            blocked_action: "connect".to_string(),
            blocked_port: 9,
            blocked_errno: errno.to_string(),
            listener_reached,
        }
    }

    fn ctx() -> RunContext {
        RunContext {
            run_binding: "a".repeat(64),
            posture_digest: "b".repeat(64),
            observed_set: "c".repeat(64),
            seal_scope: "tcp_connect_landlock_port".to_string(),
            sealed_at: "2026-08-05T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn a_blocked_run_end_probe_is_seal_eligible() {
        let h = EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(false, "EACCES")));
        let seal = build_seal_payload(&h, &ctx()).expect("a blocked probe proves still-armed");
        assert!(seal.aee_still_armed);
        assert_eq!(seal.aee_drop_count, 0);
        assert_eq!(seal.assay_drop_proof_model, DROP_PROOF_SYNCHRONOUS_PROBE);
        assert!(
            seal.aee_observed_attacks.is_empty(),
            "a signer that did not dispatch the attack must not name one"
        );
    }

    /// The load-bearing case. Without a probe the record still carries `restrict_self_confirmed`,
    /// so anything reading that field as run-end state would emit a seal here — with a guessed
    /// still-armed value and guessed zero drops.
    #[test]
    fn ruleset_applied_without_a_run_end_probe_is_refused() {
        let h = EnforcementHealthV1::landlock_active(4, vec![443], None);
        assert!(
            h.landlock.restrict_self_confirmed,
            "start-time fact is present"
        );
        let err =
            build_seal_payload(&h, &ctx()).expect_err("start-time arming is not run-end proof");
        assert_eq!(err.code(), "no-run-end-probe");
    }

    #[test]
    fn a_probe_that_reached_the_listener_is_refused() {
        let h = EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(true, "EACCES")));
        assert_eq!(
            build_seal_payload(&h, &ctx())
                .expect_err("reaching the listener is not a block")
                .code(),
            "probe-reached-listener"
        );
    }

    /// A host that is simply not listening produces `ECONNREFUSED` whether or not Landlock is armed,
    /// so crediting it would let an unenforced run seal itself.
    #[test]
    fn a_weak_probe_signal_is_refused() {
        for errno in ["ECONNREFUSED", "ETIMEDOUT", ""] {
            let h = EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(false, errno)));
            let err = build_seal_payload(&h, &ctx()).expect_err("weak signal must not seal");
            assert_eq!(err.code(), "probe-signal-too-weak", "errno {errno:?}");
        }
    }

    #[test]
    fn a_failed_run_is_refused_before_any_probe_question() {
        let h = EnforcementHealthV1 {
            status: Status::Failed,
            probe: Some(probe(false, "EACCES")),
            ..EnforcementHealthV1::landlock_active(4, vec![443], None)
        };
        assert_eq!(
            build_seal_payload(&h, &ctx())
                .expect_err("a failed arm cannot seal")
                .code(),
            "not-armed"
        );
    }

    /// Every refusal must be reachable from the builder, not only from the predicate: a builder that
    /// re-derived eligibility could drift from it, and the drift would emit seals.
    #[test]
    fn no_ineligible_input_can_reach_a_payload() {
        let cases = [
            EnforcementHealthV1::landlock_active(4, vec![443], None),
            EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(true, "EACCES"))),
            EnforcementHealthV1::landlock_active(4, vec![443], Some(probe(false, "ECONNREFUSED"))),
        ];
        for h in cases {
            assert!(seal_eligibility(&h).is_err());
            assert!(
                build_seal_payload(&h, &ctx()).is_err(),
                "builder must not outrun the predicate"
            );
        }
    }
}

#[cfg(test)]
mod checker_parity {
    use super::*;
    use crate::enforcement_health_v1::EnforcementHealthV1;

    /// The producer and the ADR-045 checker must agree on the sealed payload's member set.
    ///
    /// They are two implementations of one shape — this builder in Rust, the fixture emitter in
    /// `scripts/experiments/aee_landlock_seal_fixture.py` — and two implementations of one rule
    /// drift. Until a shared schema exists, this test is the parity fallback: it pins the member
    /// set the checker's payload-only rules read, so a field renamed on one side fails here rather
    /// than surfacing as a seal the checker silently refuses.
    ///
    /// Values are not compared. The checker's fixture is synthetic and this builder's values come
    /// from a real run; the shape is what has to match.
    #[test]
    fn the_payload_member_set_matches_the_checker_fixture() {
        let expected = [
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
            "assayDropProofModel",
            "assayNonClaims",
            "assayObservedLabels",
            "assaySealScope",
            "assaySealedAt",
            "assaySourceSchema",
        ];

        let health = EnforcementHealthV1::landlock_active(
            4,
            vec![443],
            Some(Probe {
                kind: "real_block".to_string(),
                transport: "tcp".to_string(),
                blocked_action: "connect".to_string(),
                blocked_port: 9,
                blocked_errno: "EACCES".to_string(),
                listener_reached: false,
            }),
        );
        let ctx = RunContext {
            run_binding: "a".repeat(64),
            posture_digest: "b".repeat(64),
            observed_set: "c".repeat(64),
            seal_scope: "tcp_connect_landlock_port".to_string(),
            sealed_at: "2026-08-05T00:00:00Z".to_string(),
        };
        let seal = build_seal_payload(&health, &ctx).expect("eligible");
        let value = serde_json::to_value(&seal).expect("payload serializes");
        let mut got: Vec<&str> = value
            .as_object()
            .expect("object")
            .keys()
            .map(String::as_str)
            .collect();
        got.sort_unstable();

        assert_eq!(
            got, expected,
            "producer payload members diverged from the ADR-045 checker fixture"
        );
    }

    /// The payload-only rules #2006 added to the checker, asserted on this producer's output.
    #[test]
    fn the_payload_satisfies_the_checker_payload_only_rules() {
        let health = EnforcementHealthV1::landlock_active(
            4,
            vec![443],
            Some(Probe {
                kind: "real_block".to_string(),
                transport: "tcp".to_string(),
                blocked_action: "connect".to_string(),
                blocked_port: 9,
                blocked_errno: "EACCES".to_string(),
                listener_reached: false,
            }),
        );
        let ctx = RunContext {
            run_binding: "a".repeat(64),
            posture_digest: "b".repeat(64),
            observed_set: "c".repeat(64),
            seal_scope: "tcp_connect_landlock_port".to_string(),
            sealed_at: "2026-08-05T00:00:00Z".to_string(),
        };
        let s = build_seal_payload(&health, &ctx).expect("eligible");

        assert_eq!(s.aee_kind, "sealed");
        assert_eq!(s.aee_version, "0.7");
        assert_eq!(s.aee_method, "intercepted");
        assert!(s.aee_still_armed);
        assert_eq!((s.aee_drop_count, s.aee_drop_bound), (0, 0));
        assert!(matches!(
            s.assay_drop_proof_model.as_str(),
            "synchronous-probe" | "counted-queue-zero"
        ));
        assert_eq!(
            s.assay_collection_path,
            COLLECTION_PATH_LANDLOCK_TCP_CONNECT
        );
        assert!(
            !s.assay_source_schema.is_empty(),
            "non-empty string, per the #2006 rule"
        );
        assert!(matches!(
            s.assay_attack_row_attribution_source.as_str(),
            "assembly-plane" | "substrate-runner"
        ));
        // RFC 3339 UTC instant, the shape the checker's validity-window check parses.
        assert_eq!(s.assay_sealed_at.len(), 20);
        assert!(s.assay_sealed_at.ends_with('Z'));
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
    }
}
