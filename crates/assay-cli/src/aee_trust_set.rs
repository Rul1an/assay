//! The consumer side: a trust set, and verifying a seal against it (#2093, slice D).
//!
//! # What a trust set is, and what it is not
//!
//! [`verify_seal`] takes **one** [`TrustedObservationKey`] and checks a signature against it. It
//! never decides which key to check against, and that is the right split: choosing a key is consumer
//! policy, and verifying is arithmetic. This module is the policy half.
//!
//! ADR-045 requires a trusted scope to bind at least the substrate identity, the collection path and
//! the key role, and it is explicit about what a signature alone buys:
//!
//! > a valid signature from an untrusted key is structurally valid but not credited as attested
//! > substrate evidence
//!
//! So the outcome here is three-valued, not two. A seal can be **malformed**, **structurally valid
//! but not credited**, or **credited**. Collapsing the middle case into failure would let a consumer
//! read "not credited" as "forged", and collapsing it into success is the whole thing the ADR is
//! written to prevent. `assay_landlock_seal_fixture.py` keeps the same three outcomes for the same
//! reason.
//!
//! # Key ids are lookup hints, not authority
//!
//! ADR-045 again: "key IDs are lookup hints, not authority." The keyid in an envelope selects which
//! trusted key to *try*; it never establishes anything. A seal naming a keyid nobody enrolled is not
//! credited, and a seal naming an enrolled keyid still has to verify against that key's material.
//! Slice B makes this stronger on the producer side by deriving the id from the key, but a consumer
//! cannot assume every producer does that, so nothing here treats the id as evidence.

use std::collections::BTreeMap;

use ed25519_dalek::VerifyingKey;

use crate::aee_seal::SealPayload;
use crate::aee_seal_envelope::{
    verify_seal, KeyRole, SealEnvelope, SealVerifyError, TrustedObservationKey,
};

/// The schema a trust set declares.
pub const TRUST_SET_SCHEMA: &str = "assay.aee_trust_set.v0";

const ROLE_SUBSTRATE_OBSERVATION: &str = "substrate-observation";
const ROLE_POLICY_DECISION: &str = "policy-decision";

/// Why a trust set was refused.
///
/// Refusing the *set* is different from refusing a seal: a malformed trust set means the consumer's
/// own policy could not be read, so no verdict about any seal is available. It must never degrade
/// into "nothing is trusted", which would render every seal not-credited and look like a clean run
/// of bad news.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrustSetError {
    NotStrictJson(String),
    NotAnObject,
    UnknownSchema {
        found: String,
    },
    MissingMember {
        member: &'static str,
    },
    UnknownMember {
        member: String,
    },
    /// An entry is not an object, or is missing a member an entry must carry.
    MalformedEntry {
        keyid: String,
        detail: String,
    },
    /// A public key that is not 32 bytes of base64 Ed25519.
    KeyNotEd25519 {
        keyid: String,
    },
    /// A role string this build does not know. Not silently treated as untrusted: an unrecognised
    /// role is a policy the consumer wrote and this code cannot honour.
    UnknownRole {
        keyid: String,
        found: String,
    },
    /// The same keyid enrolled twice. Refused rather than last-wins, because two entries for one id
    /// are two different policies and nothing here can choose between them.
    DuplicateKeyId {
        keyid: String,
    },
    /// A trust set with no keys. Refused, because an empty set makes every seal not-credited for a
    /// reason that has nothing to do with the seal.
    Empty,
}

impl std::fmt::Display for TrustSetError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStrictJson(d) => write!(f, "trust set is not strict JSON: {d}"),
            Self::NotAnObject => write!(f, "trust set is not a JSON object"),
            Self::UnknownSchema { found } => write!(
                f,
                "trust set declares schema {found:?}, this build implements {TRUST_SET_SCHEMA:?}"
            ),
            Self::MissingMember { member } => write!(f, "trust set has no {member:?}"),
            Self::UnknownMember { member } => {
                write!(f, "trust set carries an unknown member {member:?}")
            }
            Self::MalformedEntry { keyid, detail } => {
                write!(f, "trust set entry {keyid:?} is malformed: {detail}")
            }
            Self::KeyNotEd25519 { keyid } => {
                write!(
                    f,
                    "trust set entry {keyid:?} is not a base64 Ed25519 public key"
                )
            }
            Self::UnknownRole { keyid, found } => {
                write!(
                    f,
                    "trust set entry {keyid:?} declares an unknown role {found:?}"
                )
            }
            Self::DuplicateKeyId { keyid } => {
                write!(f, "trust set enrols {keyid:?} twice")
            }
            Self::Empty => write!(f, "trust set enrols no keys"),
        }
    }
}

impl std::error::Error for TrustSetError {}

/// What a consumer decided about one seal.
///
/// Three outcomes, because ADR-045 has three. `NotCredited` carries the reason so an operator can
/// tell "we do not know this key" from "this key is not trusted for this collection path", which are
/// different actions.
#[derive(Debug)]
pub enum SealVerdict {
    /// Signature verified and consumer policy credits the key for this payload.
    Credited(Box<SealPayload>),
    /// Structurally coherent, and not credited as attested substrate evidence.
    NotCredited(SealVerifyError),
}

impl SealVerdict {
    pub fn is_credited(&self) -> bool {
        matches!(self, Self::Credited(_))
    }
}

/// Consumer policy: which keys are trusted, for which collection paths, on which substrate.
#[derive(Debug, Clone)]
pub struct TrustSet {
    keys: BTreeMap<String, TrustedObservationKey>,
}

impl TrustSet {
    /// Parse a trust-set document.
    ///
    /// Strict, for the same reason every other boundary in this feature is: a duplicate member would
    /// let one reading be enrolled while another is displayed.
    pub fn parse(raw: &str) -> Result<Self, TrustSetError> {
        let value = assay_canonical::parse_strict(raw)
            .map_err(|e| TrustSetError::NotStrictJson(e.to_string()))?;
        let object = value.as_object().ok_or(TrustSetError::NotAnObject)?;

        match object.get("schema").and_then(|s| s.as_str()) {
            Some(TRUST_SET_SCHEMA) => {}
            other => {
                return Err(TrustSetError::UnknownSchema {
                    found: other.unwrap_or("<absent>").to_string(),
                })
            }
        }
        for key in object.keys() {
            if key != "schema" && key != "keys" {
                return Err(TrustSetError::UnknownMember {
                    member: key.clone(),
                });
            }
        }

        let entries = object
            .get("keys")
            .and_then(|k| k.as_array())
            .ok_or(TrustSetError::MissingMember { member: "keys" })?;

        let mut keys = BTreeMap::new();
        for entry in entries {
            let trusted = parse_entry(entry)?;
            if keys.contains_key(&trusted.keyid) {
                return Err(TrustSetError::DuplicateKeyId {
                    keyid: trusted.keyid,
                });
            }
            keys.insert(trusted.keyid.clone(), trusted);
        }
        if keys.is_empty() {
            return Err(TrustSetError::Empty);
        }
        Ok(Self { keys })
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Verify one envelope against this trust set.
    ///
    /// The keyid selects a candidate and nothing more; the signature still has to verify against
    /// that key's material, and the key's scope still has to cover the payload's collection path.
    /// A keyid nobody enrolled yields `NotCredited`, not an error — an unknown key is a verdict
    /// about the seal, not a failure of the consumer.
    pub fn verify(&self, envelope: &SealEnvelope) -> SealVerdict {
        let keyid = match envelope.signatures.as_slice() {
            [only] => only.keyid.clone(),
            other => return SealVerdict::NotCredited(SealVerifyError::SignatureCount(other.len())),
        };
        let Some(trusted) = self.keys.get(&keyid) else {
            return SealVerdict::NotCredited(SealVerifyError::KeyNotTrusted { keyid });
        };
        match verify_seal(envelope, trusted) {
            Ok(payload) => SealVerdict::Credited(Box::new(payload)),
            Err(e) => SealVerdict::NotCredited(e),
        }
    }

    /// The trusted key for a keyid, for a caller that also needs `check_substrate_scope`.
    pub fn get(&self, keyid: &str) -> Option<&TrustedObservationKey> {
        self.keys.get(keyid)
    }
}

fn parse_entry(entry: &serde_json::Value) -> Result<TrustedObservationKey, TrustSetError> {
    let object = entry
        .as_object()
        .ok_or_else(|| TrustSetError::MalformedEntry {
            keyid: "<unnamed>".into(),
            detail: "not an object".into(),
        })?;
    let keyid = object
        .get("keyid")
        .and_then(|v| v.as_str())
        .ok_or_else(|| TrustSetError::MalformedEntry {
            keyid: "<unnamed>".into(),
            detail: "no keyid".into(),
        })?
        .to_string();

    let named = |detail: &str| TrustSetError::MalformedEntry {
        keyid: keyid.clone(),
        detail: detail.to_string(),
    };

    let role = match object.get("role").and_then(|v| v.as_str()) {
        Some(ROLE_SUBSTRATE_OBSERVATION) => KeyRole::SubstrateObservation,
        Some(ROLE_POLICY_DECISION) => KeyRole::PolicyDecision,
        Some(other) => {
            return Err(TrustSetError::UnknownRole {
                keyid,
                found: other.to_string(),
            })
        }
        None => return Err(named("no role")),
    };

    let public_key_b64 = object
        .get("public_key")
        .and_then(|v| v.as_str())
        .ok_or_else(|| named("no public_key"))?;
    let bytes = base64_decode(public_key_b64).ok_or(TrustSetError::KeyNotEd25519 {
        keyid: keyid.clone(),
    })?;
    let array: [u8; 32] =
        bytes
            .as_slice()
            .try_into()
            .map_err(|_| TrustSetError::KeyNotEd25519 {
                keyid: keyid.clone(),
            })?;
    let verifying_key =
        VerifyingKey::from_bytes(&array).map_err(|_| TrustSetError::KeyNotEd25519 {
            keyid: keyid.clone(),
        })?;

    let collection_paths = object
        .get("collection_paths")
        .and_then(|v| v.as_array())
        .ok_or_else(|| named("no collection_paths"))?
        .iter()
        .map(|v| v.as_str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| named("collection_paths contains a non-string"))?;
    if collection_paths.is_empty() {
        // An entry trusted for no path credits nothing, so it is a policy nobody can have meant.
        return Err(named("collection_paths is empty"));
    }

    let substrate = object
        .get("substrate")
        .and_then(|v| v.as_str())
        .ok_or_else(|| named("no substrate"))?
        .to_string();

    Ok(TrustedObservationKey {
        keyid,
        role,
        verifying_key,
        collection_paths,
        substrate,
    })
}

fn base64_decode(s: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    base64::engine::general_purpose::STANDARD.decode(s).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::aee_seal_envelope::{sign_seal, PAYLOAD_TYPE};
    use base64::Engine as _;
    use ed25519_dalek::SigningKey;

    const LANDLOCK: &str = "landlock-tcp-connect";

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn b64(bytes: &[u8]) -> String {
        base64::engine::general_purpose::STANDARD.encode(bytes)
    }

    fn entry(
        keyid: &str,
        k: &SigningKey,
        role: &str,
        paths: &[&str],
        substrate: &str,
    ) -> serde_json::Value {
        serde_json::json!({
            "keyid": keyid,
            "role": role,
            "public_key": b64(k.verifying_key().as_bytes()),
            "collection_paths": paths,
            "substrate": substrate,
        })
    }

    fn trust_json(entries: Vec<serde_json::Value>) -> String {
        serde_json::json!({ "schema": TRUST_SET_SCHEMA, "keys": entries }).to_string()
    }

    /// A minimal seal, signed. The payload's own content is not what these tests are about; the
    /// collection path is, because that is what the scope check reads.
    fn signed_seal(k: &SigningKey, keyid: &str, path: &str) -> SealEnvelope {
        let mut payload = crate::aee_seal::SealPayload {
            aee_kind: "sealed".into(),
            aee_version: "0.7".into(),
            aee_run_binding: "a".repeat(64),
            aee_method: "intercepted".into(),
            aee_posture_digest: "b".repeat(64),
            aee_still_armed: true,
            aee_drop_count: 0,
            aee_drop_bound: 0,
            aee_observed_set: "c".repeat(64),
            aee_observed_attacks: vec![],
            assay_observed_labels: vec!["connect_blocked".into()],
            assay_collection_path: path.to_string(),
            assay_sealed_at: "2026-08-06T00:00:00Z".into(),
            assay_source_schema: "assay.enforcement_health.v1".into(),
            assay_seal_scope: "tcp_connect_landlock_port".into(),
            assay_drop_proof_model: "synchronous-probe".into(),
            assay_drop_proof_basis: "declared".into(),
            assay_drop_channels: vec![],
            assay_attack_row_attribution_source: "assembly-plane".into(),
            assay_non_claims: vec!["does not prove complete run population".into()],
        };
        payload.assay_collection_path = path.to_string();
        sign_seal(&payload, k, keyid, KeyRole::SubstrateObservation).expect("sign")
    }

    #[test]
    fn a_seal_from_an_enrolled_in_scope_key_is_credited() {
        let k = key(3);
        let set = TrustSet::parse(&trust_json(vec![entry(
            "k1",
            &k,
            ROLE_SUBSTRATE_OBSERVATION,
            &[LANDLOCK],
            "substrate-1",
        )]))
        .expect("parse");
        assert_eq!(set.len(), 1);
        assert!(set.verify(&signed_seal(&k, "k1", LANDLOCK)).is_credited());
    }

    /// The middle outcome. An unknown key is a verdict about the seal, not a consumer failure, and
    /// it must not be reported the same way as a bad signature.
    #[test]
    fn an_unenrolled_keyid_is_not_credited_rather_than_an_error() {
        let k = key(3);
        let set = TrustSet::parse(&trust_json(vec![entry(
            "k1",
            &k,
            ROLE_SUBSTRATE_OBSERVATION,
            &[LANDLOCK],
            "substrate-1",
        )]))
        .expect("parse");
        match set.verify(&signed_seal(&k, "someone-else", LANDLOCK)) {
            SealVerdict::NotCredited(SealVerifyError::KeyNotTrusted { keyid }) => {
                assert_eq!(keyid, "someone-else")
            }
            other => panic!("expected KeyNotTrusted, got {other:?}"),
        }
    }

    /// "Key IDs are lookup hints, not authority." An enrolled id whose material signed nothing here
    /// must not be credited on the strength of the label.
    #[test]
    fn an_enrolled_keyid_over_the_wrong_material_is_not_credited() {
        let enrolled = key(3);
        let attacker = key(9);
        let set = TrustSet::parse(&trust_json(vec![entry(
            "k1",
            &enrolled,
            ROLE_SUBSTRATE_OBSERVATION,
            &[LANDLOCK],
            "substrate-1",
        )]))
        .expect("parse");
        // Same keyid, different key.
        match set.verify(&signed_seal(&attacker, "k1", LANDLOCK)) {
            SealVerdict::NotCredited(SealVerifyError::SignatureInvalid) => {}
            other => panic!("expected SignatureInvalid, got {other:?}"),
        }
    }

    #[test]
    fn a_key_out_of_collection_path_scope_is_not_credited() {
        let k = key(3);
        let set = TrustSet::parse(&trust_json(vec![entry(
            "k1",
            &k,
            ROLE_SUBSTRATE_OBSERVATION,
            &["json-rpc-proxy"],
            "substrate-1",
        )]))
        .expect("parse");
        match set.verify(&signed_seal(&k, "k1", LANDLOCK)) {
            SealVerdict::NotCredited(SealVerifyError::CollectionPathOutOfScope { path }) => {
                assert_eq!(path, LANDLOCK)
            }
            other => panic!("expected CollectionPathOutOfScope, got {other:?}"),
        }
    }

    /// ADR-045: policy-decision keys must not sign substrate observations. Enrolling one is allowed
    /// — a consumer may hold both roles — but crediting a seal to it is not.
    #[test]
    fn a_policy_decision_key_never_credits_a_seal() {
        let k = key(3);
        let set = TrustSet::parse(&trust_json(vec![entry(
            "k1",
            &k,
            ROLE_POLICY_DECISION,
            &[LANDLOCK],
            "substrate-1",
        )]))
        .expect("parse");
        match set.verify(&signed_seal(&k, "k1", LANDLOCK)) {
            SealVerdict::NotCredited(SealVerifyError::WrongKeyRole) => {}
            other => panic!("expected WrongKeyRole, got {other:?}"),
        }
    }

    /// Two entries for one id are two policies, and nothing here can choose. Last-wins would let the
    /// later entry silently widen the earlier one's scope.
    #[test]
    fn a_duplicate_keyid_is_refused() {
        let k = key(3);
        let raw = trust_json(vec![
            entry("k1", &k, ROLE_SUBSTRATE_OBSERVATION, &[LANDLOCK], "s1"),
            entry(
                "k1",
                &k,
                ROLE_SUBSTRATE_OBSERVATION,
                &["json-rpc-proxy"],
                "s1",
            ),
        ]);
        assert_eq!(
            TrustSet::parse(&raw).unwrap_err(),
            TrustSetError::DuplicateKeyId {
                keyid: "k1".to_string()
            }
        );
    }

    /// An empty set would make every seal not-credited for a reason that has nothing to do with any
    /// seal, which reads like a clean run of bad news.
    #[test]
    fn an_empty_trust_set_is_refused() {
        assert_eq!(
            TrustSet::parse(&trust_json(vec![])).unwrap_err(),
            TrustSetError::Empty
        );
    }

    #[test]
    fn an_entry_trusted_for_no_path_is_refused() {
        let k = key(3);
        let raw = trust_json(vec![entry("k1", &k, ROLE_SUBSTRATE_OBSERVATION, &[], "s1")]);
        assert!(matches!(
            TrustSet::parse(&raw),
            Err(TrustSetError::MalformedEntry { .. })
        ));
    }

    /// An unrecognised role is a policy this build cannot honour. Treating it as untrusted would
    /// silently downgrade a consumer's decision rather than telling them it was not understood.
    #[test]
    fn an_unknown_role_is_refused_not_downgraded() {
        let k = key(3);
        let raw = trust_json(vec![entry("k1", &k, "auditor", &[LANDLOCK], "s1")]);
        assert_eq!(
            TrustSet::parse(&raw).unwrap_err(),
            TrustSetError::UnknownRole {
                keyid: "k1".to_string(),
                found: "auditor".to_string()
            }
        );
    }

    #[test]
    fn a_duplicate_member_in_the_document_is_refused() {
        let k = key(3);
        let raw = trust_json(vec![entry(
            "k1",
            &k,
            ROLE_SUBSTRATE_OBSERVATION,
            &[LANDLOCK],
            "s1",
        )]);
        let doubled = raw.replacen("\"keys\":", "\"keys\":[],\"keys\":", 1);
        assert!(serde_json::from_str::<serde_json::Value>(&doubled).is_ok());
        assert!(matches!(
            TrustSet::parse(&doubled),
            Err(TrustSetError::NotStrictJson(_))
        ));
    }

    #[test]
    fn a_bad_public_key_is_refused() {
        let raw = trust_json(vec![serde_json::json!({
            "keyid": "k1",
            "role": ROLE_SUBSTRATE_OBSERVATION,
            "public_key": "not base64!!",
            "collection_paths": [LANDLOCK],
            "substrate": "s1",
        })]);
        assert_eq!(
            TrustSet::parse(&raw).unwrap_err(),
            TrustSetError::KeyNotEd25519 {
                keyid: "k1".to_string()
            }
        );
    }

    #[test]
    fn a_wrong_payload_type_is_not_credited() {
        let k = key(3);
        let set = TrustSet::parse(&trust_json(vec![entry(
            "k1",
            &k,
            ROLE_SUBSTRATE_OBSERVATION,
            &[LANDLOCK],
            "substrate-1",
        )]))
        .expect("parse");
        let mut env = signed_seal(&k, "k1", LANDLOCK);
        env.payload_type = "application/vnd.something.else+json".into();
        assert_ne!(env.payload_type, PAYLOAD_TYPE);
        match set.verify(&env) {
            SealVerdict::NotCredited(SealVerifyError::UnsupportedPayloadType(_)) => {}
            other => panic!("expected UnsupportedPayloadType, got {other:?}"),
        }
    }

    #[test]
    fn a_missing_or_wrong_schema_is_refused() {
        assert!(matches!(
            TrustSet::parse(r#"{"keys":[]}"#),
            Err(TrustSetError::UnknownSchema { .. })
        ));
        assert!(matches!(
            TrustSet::parse(r#"{"schema":"assay.aee_trust_set.v1","keys":[]}"#),
            Err(TrustSetError::UnknownSchema { .. })
        ));
    }
}
