//! Loading the substrate observation key a run signs its seal with (#2093, slice B).
//!
//! # The check that is deliberately absent, and why
//!
//! The design note for this slice said it would "refuse fixture scope". It does not, and that is a
//! finding rather than an omission.
//!
//! The fixture checker signs with HMAC-SHA256 (`aee_landlock_seal_fixture.py:196`:
//! `hmac.new(SECRET + keyid, pae(...), sha256)`), and its `fixture-key-in-production-path` rule
//! fires on a *declared keyid* carrying the prefix `assay-aee-spike-fixture-key`. There is no
//! Ed25519 fixture key. A producer built on [`ed25519_dalek`] cannot load one, cannot sign with one,
//! and cannot accidentally emit a statement that trips that rule.
//!
//! Writing a fixture-key check here would therefore add a branch that can never be taken — which is
//! the exact shape this repository has spent a week removing: #2076's gate that returned 0 on every
//! path, #1993's release gate whose only test was cutting a release, #2088's semver job that skipped
//! all 254 checks. A check that cannot fire is worse than no check, because it reads like coverage.
//!
//! What removes the risk instead is the keyid never being a caller's to choose. See below.
//!
//! # The keyid is derived, not declared
//!
//! `compute_key_id_from_verifying_key` (`assay-evidence`) is this workspace's convention:
//! `sha256:<hex of the SPKI DER>`. Using it here means:
//!
//! - a caller cannot label a key as something it is not, because there is no field to label it in;
//! - a consumer's trust-set entry names key *material*, not a string the producer picked;
//! - the fixture prefix cannot appear in our output by construction, so the checker's rule stays
//!   meaningful for statements from producers that do choose their own ids.
//!
//! The fixture corpus uses free-form ids (`assay-test-observation-key-landlock-v0`), and that is
//! fine: those are a consumer trust set's business, and a consumer can enrol a `sha256:` id exactly
//! as easily. A key file that tries to declare an id is rejected as an unknown member rather than
//! ignored, so nobody is left believing they set something.
//!
//! # What this does check
//!
//! The role, and only because the role is a claim the file makes about a key rather than a property
//! of the bytes. ADR-045: "policy-decision keys MUST NOT sign substrate observation records."
//! `sign_seal` refuses a non-observation role too, so this is the earlier of two gates — the point
//! is to fail while a human is still looking at the key file, not at signing time.

use std::path::Path;

use ed25519_dalek::pkcs8::DecodePrivateKey;
use ed25519_dalek::SigningKey;

use crate::aee_seal_envelope::KeyRole;

/// The schema a key descriptor declares. Versioned so it cannot become an implicit format.
pub const SEAL_KEY_SCHEMA: &str = "assay.aee_seal_key.v0";

/// The one role permitted to sign a substrate observation, as spelled in the descriptor.
const ROLE_SUBSTRATE_OBSERVATION: &str = "substrate-observation";

/// Why a key descriptor was refused.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SealKeyError {
    /// The descriptor is not strict JSON, or carries a duplicate key at any depth.
    NotStrictJson(String),
    /// The top level is not a JSON object.
    NotAnObject,
    /// `schema` is absent or names a format this build does not implement.
    UnknownSchema { found: String },
    /// A member the descriptor must carry is absent.
    MissingMember { member: &'static str },
    /// A member this descriptor does not define. `keyid` lands here on purpose: it is derived from
    /// the key material, so a declared one would be a value the caller believes they set and this
    /// code ignores.
    UnknownMember { member: String },
    /// `role` names something other than a substrate observation key. ADR-045 forbids a
    /// policy-decision key from signing a substrate observation record.
    NotAnObservationRole { found: String },
    /// The private key file could not be read.
    KeyUnreadable { detail: String },
    /// The private key is not a PKCS#8 Ed25519 key.
    KeyNotEd25519Pkcs8 { detail: String },
    /// The key id could not be derived from the key material.
    KeyIdNotDerivable { detail: String },
}

impl std::fmt::Display for SealKeyError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStrictJson(d) => write!(f, "seal key descriptor is not strict JSON: {d}"),
            Self::NotAnObject => write!(f, "seal key descriptor is not a JSON object"),
            Self::UnknownSchema { found } => write!(
                f,
                "seal key descriptor declares schema {found:?}, this build implements {SEAL_KEY_SCHEMA:?}"
            ),
            Self::MissingMember { member } => {
                write!(f, "seal key descriptor has no {member:?}")
            }
            Self::UnknownMember { member } => write!(
                f,
                "seal key descriptor carries an unknown member {member:?}; the key id is derived from the key material and cannot be declared"
            ),
            Self::NotAnObservationRole { found } => write!(
                f,
                "seal key descriptor declares role {found:?}; only {ROLE_SUBSTRATE_OBSERVATION:?} may sign a substrate observation"
            ),
            Self::KeyUnreadable { detail } => write!(f, "seal key file unreadable: {detail}"),
            Self::KeyNotEd25519Pkcs8 { detail } => {
                write!(f, "seal key is not a PKCS#8 Ed25519 private key: {detail}")
            }
            Self::KeyIdNotDerivable { detail } => {
                write!(f, "seal key id could not be derived: {detail}")
            }
        }
    }
}

impl std::error::Error for SealKeyError {}

/// A loaded substrate observation key, ready for `sign_seal`.
///
/// No `Debug` that could print the private half: `SigningKey`'s own `Debug` is redacted, but the
/// derivation is stated here so nobody adds a field that is not.
pub struct SealSigningKey {
    signing_key: SigningKey,
    keyid: String,
}

impl std::fmt::Debug for SealSigningKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SealSigningKey")
            .field("keyid", &self.keyid)
            .field("signing_key", &"<redacted>")
            .finish()
    }
}

impl SealSigningKey {
    /// `sha256:<hex>` over the SPKI DER of the public half.
    pub fn keyid(&self) -> &str {
        &self.keyid
    }

    pub fn signing_key(&self) -> &SigningKey {
        &self.signing_key
    }

    /// The role, which is always [`KeyRole::SubstrateObservation`] because loading refuses anything
    /// else. Returned so a caller passes a value rather than re-asserting a constant.
    pub fn role(&self) -> KeyRole {
        KeyRole::SubstrateObservation
    }
}

/// Load a key descriptor and the private key it points at.
///
/// `private_key_path` is resolved relative to the descriptor's own directory, so a descriptor and
/// its key move together and a relative path never silently resolves against the process's working
/// directory.
pub fn load(descriptor_path: &Path, raw: &str) -> Result<SealSigningKey, SealKeyError> {
    let value = assay_canonical::parse_strict(raw)
        .map_err(|e| SealKeyError::NotStrictJson(e.to_string()))?;
    let object = value.as_object().ok_or(SealKeyError::NotAnObject)?;

    match object.get("schema").and_then(|s| s.as_str()) {
        Some(SEAL_KEY_SCHEMA) => {}
        other => {
            return Err(SealKeyError::UnknownSchema {
                found: other.unwrap_or("<absent>").to_string(),
            })
        }
    }

    const KNOWN: [&str; 3] = ["schema", "role", "private_key_path"];
    for key in object.keys() {
        if !KNOWN.contains(&key.as_str()) {
            return Err(SealKeyError::UnknownMember {
                member: key.clone(),
            });
        }
    }

    let role = object
        .get("role")
        .and_then(|r| r.as_str())
        .ok_or(SealKeyError::MissingMember { member: "role" })?;
    if role != ROLE_SUBSTRATE_OBSERVATION {
        return Err(SealKeyError::NotAnObservationRole {
            found: role.to_string(),
        });
    }

    let rel = object
        .get("private_key_path")
        .and_then(|p| p.as_str())
        .ok_or(SealKeyError::MissingMember {
            member: "private_key_path",
        })?;
    let key_path = descriptor_path
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(rel);

    let pem = std::fs::read_to_string(&key_path).map_err(|e| SealKeyError::KeyUnreadable {
        detail: format!("{}: {e}", key_path.display()),
    })?;
    let signing_key =
        SigningKey::from_pkcs8_pem(&pem).map_err(|e| SealKeyError::KeyNotEd25519Pkcs8 {
            detail: e.to_string(),
        })?;

    let keyid = assay_evidence::mandate::signing::compute_key_id_from_verifying_key(
        &signing_key.verifying_key(),
    )
    .map_err(|e| SealKeyError::KeyIdNotDerivable {
        detail: e.to_string(),
    })?;

    Ok(SealSigningKey { signing_key, keyid })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::pkcs8::EncodePrivateKey;

    fn write_key(dir: &Path, name: &str, seed: [u8; 32]) -> SigningKey {
        let key = SigningKey::from_bytes(&seed);
        let pem = key
            .to_pkcs8_pem(ed25519_dalek::pkcs8::spki::der::pem::LineEnding::LF)
            .expect("encode");
        std::fs::write(dir.join(name), pem.as_bytes()).expect("write");
        key
    }

    fn descriptor(role: &str, key_name: &str) -> String {
        serde_json::json!({
            "schema": SEAL_KEY_SCHEMA,
            "role": role,
            "private_key_path": key_name,
        })
        .to_string()
    }

    fn load_in(dir: &Path, raw: &str) -> Result<SealSigningKey, SealKeyError> {
        load(&dir.join("key.json"), raw)
    }

    #[test]
    fn a_substrate_observation_key_loads_and_derives_its_own_id() {
        let dir = tempfile::tempdir().expect("tmp");
        let key = write_key(dir.path(), "k.pem", [3u8; 32]);
        let loaded =
            load_in(dir.path(), &descriptor(ROLE_SUBSTRATE_OBSERVATION, "k.pem")).expect("load");

        assert_eq!(loaded.role(), KeyRole::SubstrateObservation);
        assert_eq!(
            loaded.signing_key().verifying_key(),
            key.verifying_key(),
            "loaded a different key than was written"
        );

        // Derived, not declared: the same material always yields the same id, and the descriptor
        // has nowhere to put a different one.
        let expected = assay_evidence::mandate::signing::compute_key_id_from_verifying_key(
            &key.verifying_key(),
        )
        .expect("derive");
        assert_eq!(loaded.keyid(), expected);
        assert!(loaded.keyid().starts_with("sha256:"), "{}", loaded.keyid());
    }

    /// Two different keys must not share an id, or a consumer trust set cannot tell them apart.
    #[test]
    fn different_key_material_yields_different_ids() {
        let dir = tempfile::tempdir().expect("tmp");
        write_key(dir.path(), "a.pem", [3u8; 32]);
        write_key(dir.path(), "b.pem", [4u8; 32]);
        let a = load_in(dir.path(), &descriptor(ROLE_SUBSTRATE_OBSERVATION, "a.pem")).expect("a");
        let b = load_in(dir.path(), &descriptor(ROLE_SUBSTRATE_OBSERVATION, "b.pem")).expect("b");
        assert_ne!(a.keyid(), b.keyid());
    }

    /// ADR-045: policy-decision keys MUST NOT sign substrate observation records. Refused here so
    /// the failure lands while someone is looking at the key file, not at signing time.
    #[test]
    fn a_policy_decision_role_is_refused() {
        let dir = tempfile::tempdir().expect("tmp");
        write_key(dir.path(), "k.pem", [3u8; 32]);
        assert_eq!(
            load_in(dir.path(), &descriptor("policy-decision", "k.pem")).unwrap_err(),
            SealKeyError::NotAnObservationRole {
                found: "policy-decision".to_string()
            }
        );
    }

    /// A declared keyid is refused rather than ignored. Silently dropping it would leave a caller
    /// believing they had enrolled an id that nothing reads.
    #[test]
    fn a_declared_keyid_is_refused_not_ignored() {
        let dir = tempfile::tempdir().expect("tmp");
        write_key(dir.path(), "k.pem", [3u8; 32]);
        let raw = serde_json::json!({
            "schema": SEAL_KEY_SCHEMA,
            "role": ROLE_SUBSTRATE_OBSERVATION,
            "private_key_path": "k.pem",
            "keyid": "assay-aee-spike-fixture-key-v0",
        })
        .to_string();
        assert_eq!(
            load_in(dir.path(), &raw).unwrap_err(),
            SealKeyError::UnknownMember {
                member: "keyid".to_string()
            }
        );
    }

    /// The fixture signer is HMAC-SHA256, not Ed25519, so no fixture key can be loaded here at all.
    ///
    /// This is why there is no fixture-key check in `load`: it would be a branch that can never be
    /// taken. The assertion is on the property that makes it unnecessary — Ed25519 PKCS#8 parsing
    /// rejects the fixture's shared secret — so if that ever stops being true, this fails and the
    /// missing check becomes visible.
    #[test]
    fn the_fixture_signing_secret_is_not_loadable_as_a_key() {
        let dir = tempfile::tempdir().expect("tmp");
        std::fs::write(
            dir.path().join("k.pem"),
            b"assay-aee-landlock-seal-fixture-key-not-production",
        )
        .expect("write");
        assert!(matches!(
            load_in(dir.path(), &descriptor(ROLE_SUBSTRATE_OBSERVATION, "k.pem")),
            Err(SealKeyError::KeyNotEd25519Pkcs8 { .. })
        ));
    }

    #[test]
    fn a_duplicate_key_in_the_descriptor_is_refused() {
        let dir = tempfile::tempdir().expect("tmp");
        write_key(dir.path(), "k.pem", [3u8; 32]);
        let raw = format!(
            "{{\"schema\":\"{SEAL_KEY_SCHEMA}\",\"role\":\"{ROLE_SUBSTRATE_OBSERVATION}\",\"private_key_path\":\"a.pem\",\"private_key_path\":\"k.pem\"}}"
        );
        assert!(serde_json::from_str::<serde_json::Value>(&raw).is_ok());
        assert!(matches!(
            load_in(dir.path(), &raw),
            Err(SealKeyError::NotStrictJson(_))
        ));
    }

    #[test]
    fn a_missing_member_or_schema_is_refused() {
        let dir = tempfile::tempdir().expect("tmp");
        write_key(dir.path(), "k.pem", [3u8; 32]);

        let no_role =
            serde_json::json!({"schema": SEAL_KEY_SCHEMA, "private_key_path": "k.pem"}).to_string();
        assert_eq!(
            load_in(dir.path(), &no_role).unwrap_err(),
            SealKeyError::MissingMember { member: "role" }
        );

        let no_path =
            serde_json::json!({"schema": SEAL_KEY_SCHEMA, "role": ROLE_SUBSTRATE_OBSERVATION})
                .to_string();
        assert_eq!(
            load_in(dir.path(), &no_path).unwrap_err(),
            SealKeyError::MissingMember {
                member: "private_key_path"
            }
        );

        let wrong_schema = serde_json::json!({
            "schema": "assay.aee_seal_key.v1",
            "role": ROLE_SUBSTRATE_OBSERVATION,
            "private_key_path": "k.pem"
        })
        .to_string();
        assert!(matches!(
            load_in(dir.path(), &wrong_schema),
            Err(SealKeyError::UnknownSchema { .. })
        ));
    }

    /// The key path resolves against the descriptor, not the process's working directory, so a
    /// descriptor and its key travel together.
    #[test]
    fn the_key_path_resolves_relative_to_the_descriptor() {
        let dir = tempfile::tempdir().expect("tmp");
        let nested = dir.path().join("nested");
        std::fs::create_dir(&nested).expect("mkdir");
        write_key(&nested, "k.pem", [3u8; 32]);

        // Descriptor lives in `nested/`, so a bare "k.pem" must find `nested/k.pem`.
        assert!(load(
            &nested.join("key.json"),
            &descriptor(ROLE_SUBSTRATE_OBSERVATION, "k.pem")
        )
        .is_ok());

        // And a descriptor at the root must not find it.
        assert!(matches!(
            load_in(dir.path(), &descriptor(ROLE_SUBSTRATE_OBSERVATION, "k.pem")),
            Err(SealKeyError::KeyUnreadable { .. })
        ));
    }

    /// The private half never reaches a log line through `Debug`.
    #[test]
    fn debug_does_not_print_the_private_key() {
        let dir = tempfile::tempdir().expect("tmp");
        write_key(dir.path(), "k.pem", [3u8; 32]);
        let loaded =
            load_in(dir.path(), &descriptor(ROLE_SUBSTRATE_OBSERVATION, "k.pem")).expect("load");
        let rendered = format!("{loaded:?}");
        assert!(rendered.contains("<redacted>"), "{rendered}");
        assert!(!rendered.contains("030303"), "{rendered}");
    }
}
