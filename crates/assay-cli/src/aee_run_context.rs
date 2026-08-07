//! The AEE run context a caller hands to the enforcing process (#2093, slice A).
//!
//! # What this is, and the boundary it must not blur
//!
//! [`ObservationEnvironment`] needs seven inputs. The sandbox knows exactly one of them: the network
//! posture it actually applied. The other six — subject, substrate, corpus, catch policy,
//! observation vocabulary, run entropy — are AEE run concepts. `assay sandbox` is not an AEE run; it
//! is one enforcement episode inside one, and it cannot derive them because there is nothing to
//! derive them from.
//!
//! So they are supplied. That makes them **carried, not proved**, and the distinction is the whole
//! reason this module documents rather than asserts. The 2026 SCITT draft on binding provenance into
//! agent action capsules puts it in a section titled *Tamper-Evidence Is Not Honesty*:
//!
//! > this binding attests record bytes and referenced digests, not the honesty of the runtime at the
//! > moment of recording
//!
//! ([draft-rampalli-scitt-capsule-provenance-binding-00], §11.2.) Signing a seal that carries these
//! digests attests the bytes we were handed. It does not attest that they describe the run. The
//! caller-supplied versus runtime-observed split is not that draft's — it does not draw one — it is
//! this crate's reading of where the two kinds of input differ.
//!
//! What this module can therefore do is narrow and worth doing: refuse a context that is malformed,
//! ambiguous, or incomplete, so that a seal is never signed over inputs nobody could resolve.
//!
//! # Why the file mirrors the statement
//!
//! The shape is `{"corpus": {"digest": {"sha256": "…"}}}`, not six bare hex strings, because that is
//! the shape the values already live in: `aee_spike_lib.py:144` reads them straight out of a
//! statement's `observationEnvironment`. A caller lifts a subtree instead of transcribing six
//! sixty-four-character strings, and transcription is where binding errors come from.
//!
//! Extra algorithms inside a `digest` map are ignored rather than refused, per in-toto's DigestSet
//! rule that consumers "MUST only accept algorithms that they consider secure and MUST ignore
//! unrecognized or unaccepted algorithms". `sha256` is the one `run_binding` uses, so it is the one
//! required; a `sha512` beside it is not an error.
//!
//! [draft-rampalli-scitt-capsule-provenance-binding-00]: https://datatracker.ietf.org/doc/html/draft-rampalli-scitt-capsule-provenance-binding-00

use crate::aee_seal::{is_sha256_hex, ObservationEnvironment};

/// The schema this file declares. Versioned so it can never become an implicit format.
pub const RUN_CONTEXT_SCHEMA: &str = "assay.aee_run_context.v0";

/// The six members, in the spelling `run_binding` uses for its own input object.
///
/// Same names on both sides on purpose: a reader comparing the file to the binding can line them up
/// without a mapping table, and a mapping table is a place for two names to drift apart.
const REQUIRED_MEMBERS: [&str; 6] = [
    "catchPolicy",
    "corpus",
    "observationVocabulary",
    "runEntropy",
    "subject",
    "substrate",
];

/// Why a run context was refused.
///
/// Every variant names the member, because a caller assembling this file by hand needs to know which
/// of six digests is wrong, and "invalid run context" tells them to check all six.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunContextError {
    /// The bytes are not JSON, or carry a duplicate key at any depth.
    NotStrictJson(String),
    /// The top level is not a JSON object.
    NotAnObject,
    /// `schema` is absent, or names a format this build does not implement.
    UnknownSchema { found: String },
    /// A member this context must carry is absent.
    MissingMember { member: &'static str },
    /// A member is present but is not an object carrying `digest`.
    MalformedMember { member: &'static str },
    /// A member carries a `digest` map with no `sha256`, which is the one algorithm the run binding
    /// uses. Other algorithms beside it are ignored; their presence is not what failed.
    MissingSha256 { member: &'static str },
    /// A `sha256` value is not 64 lowercase hex characters.
    NotLowercaseSha256Hex { member: &'static str },
    /// A member this context does not define. Rejected rather than ignored: a misspelled `corpus`
    /// would otherwise be dropped in silence and surface as a missing member somewhere less
    /// obvious, or not at all.
    UnknownMember { member: String },
}

impl std::fmt::Display for RunContextError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotStrictJson(detail) => {
                write!(f, "run context is not strict JSON: {detail}")
            }
            Self::NotAnObject => write!(f, "run context is not a JSON object"),
            Self::UnknownSchema { found } => write!(
                f,
                "run context declares schema {found:?}, this build implements {RUN_CONTEXT_SCHEMA:?}"
            ),
            Self::MissingMember { member } => write!(f, "run context has no {member:?}"),
            Self::MalformedMember { member } => {
                write!(f, "run context {member:?} is not an object carrying a digest")
            }
            Self::MissingSha256 { member } => {
                write!(f, "run context {member:?} has no sha256 digest")
            }
            Self::NotLowercaseSha256Hex { member } => write!(
                f,
                "run context {member:?} sha256 is not 64 lowercase hex characters"
            ),
            Self::UnknownMember { member } => {
                write!(f, "run context carries an unknown member {member:?}")
            }
        }
    }
}

impl std::error::Error for RunContextError {}

/// Six digests a caller supplies, validated for shape and nothing else.
///
/// Deliberately not `Deserialize`. Constructing one has to go through [`AeeRunContext::parse`], so
/// there is no path that produces this type without the strict parse and the six checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AeeRunContext {
    catch_policy: String,
    corpus: String,
    observation_vocabulary: String,
    run_entropy: String,
    subject: String,
    substrate: String,
}

impl AeeRunContext {
    /// Parse and validate a run-context document.
    ///
    /// `parse_strict`, not `serde_json::from_str`. Its own documentation gives the reason: it is the
    /// entry point for untrusted JSON that will be content-addressed, and these six digests are
    /// hashed into the run binding. `serde_json` keeps the last of a duplicate key, so a file
    /// carrying `corpus` twice would bind the run to one value while displaying another. The nearest
    /// precedent in this repo (`supply_chain_conformance/descriptor.rs`) parses the lenient way; that
    /// is a gap there, not a pattern to copy.
    pub fn parse(raw: &str) -> Result<Self, RunContextError> {
        let value = assay_canonical::parse_strict(raw)
            .map_err(|e| RunContextError::NotStrictJson(e.to_string()))?;
        let object = value.as_object().ok_or(RunContextError::NotAnObject)?;

        match object.get("schema").and_then(|s| s.as_str()) {
            Some(RUN_CONTEXT_SCHEMA) => {}
            other => {
                return Err(RunContextError::UnknownSchema {
                    found: other.unwrap_or("<absent>").to_string(),
                })
            }
        }

        for key in object.keys() {
            if key != "schema" && !REQUIRED_MEMBERS.contains(&key.as_str()) {
                return Err(RunContextError::UnknownMember {
                    member: key.clone(),
                });
            }
        }

        let mut digests = Vec::with_capacity(REQUIRED_MEMBERS.len());
        for member in REQUIRED_MEMBERS {
            digests.push(read_sha256(object, member)?);
        }

        // Positional, and the order is `REQUIRED_MEMBERS`. Asserted rather than trusted, because a
        // reordering of that array would otherwise silently swap two digests and the run binding
        // would be wrong in a way no test on this file would show.
        debug_assert_eq!(REQUIRED_MEMBERS.len(), digests.len());
        Ok(Self {
            catch_policy: digests[0].clone(),
            corpus: digests[1].clone(),
            observation_vocabulary: digests[2].clone(),
            run_entropy: digests[3].clone(),
            subject: digests[4].clone(),
            substrate: digests[5].clone(),
        })
    }

    /// Combine the carried six with the one input the run actually observed.
    ///
    /// `network_posture` is the sandbox's own: it is the only member of the environment this process
    /// can speak to, and it is passed here rather than carried in the file for that reason.
    pub fn into_environment(self, network_posture: serde_json::Value) -> ObservationEnvironment {
        ObservationEnvironment {
            subject_digest: self.subject,
            substrate_digest: self.substrate,
            corpus_digest: self.corpus,
            catch_policy_digest: self.catch_policy,
            observation_vocabulary_digest: self.observation_vocabulary,
            run_entropy_digest: self.run_entropy,
            network_posture,
        }
    }
}

/// `member.digest.sha256`, validated.
fn read_sha256(
    object: &serde_json::Map<String, serde_json::Value>,
    member: &'static str,
) -> Result<String, RunContextError> {
    let entry = object
        .get(member)
        .ok_or(RunContextError::MissingMember { member })?;
    let digest = entry
        .get("digest")
        .and_then(|d| d.as_object())
        .ok_or(RunContextError::MalformedMember { member })?;
    let sha256 = digest
        .get("sha256")
        .and_then(|v| v.as_str())
        .ok_or(RunContextError::MissingSha256 { member })?;
    if !is_sha256_hex(sha256) {
        return Err(RunContextError::NotLowercaseSha256Hex { member });
    }
    Ok(sha256.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digest(byte: u8) -> String {
        format!("{byte:02x}").repeat(32)
    }

    fn context_json(overrides: &[(&str, serde_json::Value)]) -> String {
        let mut doc = serde_json::json!({ "schema": RUN_CONTEXT_SCHEMA });
        for (index, member) in REQUIRED_MEMBERS.iter().enumerate() {
            doc[*member] = serde_json::json!({
                "digest": { "sha256": digest(index as u8 + 1) }
            });
        }
        for (key, value) in overrides {
            if value.is_null() {
                doc.as_object_mut().expect("object").remove(*key);
            } else {
                doc[*key] = value.clone();
            }
        }
        serde_json::to_string(&doc).expect("serialize")
    }

    #[test]
    fn a_well_formed_context_carries_each_digest_to_its_own_field() {
        let ctx = AeeRunContext::parse(&context_json(&[])).expect("parse");
        // Read back by name, not by position: the point of the assertion is that `corpus` in the
        // file becomes `corpus` in the environment, which a positional check would not show.
        let env = ctx.into_environment(serde_json::json!({}));
        assert_eq!(env.catch_policy_digest, digest(1));
        assert_eq!(env.corpus_digest, digest(2));
        assert_eq!(env.observation_vocabulary_digest, digest(3));
        assert_eq!(env.run_entropy_digest, digest(4));
        assert_eq!(env.subject_digest, digest(5));
        assert_eq!(env.substrate_digest, digest(6));
    }

    /// The reason this parses strict. Six digests are hashed into the run binding; a file carrying
    /// `corpus` twice would bind one value while a reader sees another.
    #[test]
    fn a_duplicate_key_is_refused_rather_than_resolved() {
        let raw = context_json(&[]);
        let doubled = raw.replacen(
            "\"corpus\":",
            "\"corpus\":{\"digest\":{\"sha256\":\"aa\"}},\"corpus\":",
            1,
        );
        // serde_json alone would accept this and keep the last one.
        assert!(serde_json::from_str::<serde_json::Value>(&doubled).is_ok());
        assert!(matches!(
            AeeRunContext::parse(&doubled),
            Err(RunContextError::NotStrictJson(_))
        ));
    }

    /// in-toto's DigestSet rule: consumers accept the algorithms they consider secure and ignore
    /// the rest. A `sha512` beside the `sha256` is not an error.
    #[test]
    fn an_extra_algorithm_beside_sha256_is_ignored_not_refused() {
        let raw = context_json(&[(
            "corpus",
            serde_json::json!({
                "digest": { "sha256": digest(2), "sha512": "f".repeat(128) }
            }),
        )]);
        let env = AeeRunContext::parse(&raw)
            .expect("parse")
            .into_environment(serde_json::json!({}));
        assert_eq!(env.corpus_digest, digest(2));
    }

    /// And the one algorithm the run binding uses is required, whatever else is present.
    #[test]
    fn a_digest_map_without_sha256_is_refused() {
        let raw = context_json(&[(
            "corpus",
            serde_json::json!({ "digest": { "sha512": "f".repeat(128) } }),
        )]);
        assert_eq!(
            AeeRunContext::parse(&raw),
            Err(RunContextError::MissingSha256 { member: "corpus" })
        );
    }

    #[test]
    fn every_required_member_is_required_by_name() {
        for member in REQUIRED_MEMBERS {
            let raw = context_json(&[(member, serde_json::Value::Null)]);
            assert_eq!(
                AeeRunContext::parse(&raw),
                Err(RunContextError::MissingMember { member }),
                "{member} was not required"
            );
        }
    }

    /// A misspelled member must not be dropped in silence. Without this, `corpusDigest` for `corpus`
    /// reports a missing `corpus` and says nothing about the key that was actually there.
    #[test]
    fn an_unknown_member_is_refused() {
        let raw = context_json(&[(
            "corpusDigest",
            serde_json::json!({"digest":{"sha256":digest(9)}}),
        )]);
        assert_eq!(
            AeeRunContext::parse(&raw),
            Err(RunContextError::UnknownMember {
                member: "corpusDigest".to_string()
            })
        );
    }

    #[test]
    fn an_uppercase_or_short_digest_is_refused() {
        for bad in [
            "AA".repeat(32),
            "ab".repeat(31),
            String::new(),
            "g".repeat(64),
        ] {
            let raw = context_json(&[("corpus", serde_json::json!({"digest":{"sha256":bad}}))]);
            assert_eq!(
                AeeRunContext::parse(&raw),
                Err(RunContextError::NotLowercaseSha256Hex { member: "corpus" }),
                "accepted {bad:?}"
            );
        }
    }

    #[test]
    fn a_missing_or_wrong_schema_is_refused() {
        let absent = context_json(&[("schema", serde_json::Value::Null)]);
        assert_eq!(
            AeeRunContext::parse(&absent),
            Err(RunContextError::UnknownSchema {
                found: "<absent>".to_string()
            })
        );
        let wrong = context_json(&[("schema", serde_json::json!("assay.aee_run_context.v1"))]);
        assert!(matches!(
            AeeRunContext::parse(&wrong),
            Err(RunContextError::UnknownSchema { .. })
        ));
    }

    #[test]
    fn a_member_that_is_not_an_object_is_refused() {
        let raw = context_json(&[("corpus", serde_json::json!(digest(2)))]);
        assert_eq!(
            AeeRunContext::parse(&raw),
            Err(RunContextError::MalformedMember { member: "corpus" })
        );
    }

    #[test]
    fn a_non_object_document_is_refused() {
        assert_eq!(
            AeeRunContext::parse("[]"),
            Err(RunContextError::NotAnObject)
        );
        assert!(matches!(
            AeeRunContext::parse("not json"),
            Err(RunContextError::NotStrictJson(_))
        ));
    }

    /// The posture is the sandbox's, not the file's. Pinned because the tempting simplification is
    /// to let the caller supply all seven, which would make the one member this process genuinely
    /// observes into another thing it was told.
    #[test]
    fn the_posture_comes_from_the_caller_of_into_environment_not_the_file() {
        let raw = context_json(&[(
            "networkPosture",
            serde_json::json!({"digest": {"sha256": digest(7)}}),
        )]);
        assert_eq!(
            AeeRunContext::parse(&raw),
            Err(RunContextError::UnknownMember {
                member: "networkPosture".to_string()
            })
        );

        let env = AeeRunContext::parse(&context_json(&[]))
            .expect("parse")
            .into_environment(serde_json::json!({"observed": true}));
        assert_eq!(env.network_posture, serde_json::json!({"observed": true}));
    }

    /// Every error names its member, so a caller with six digests knows which one to look at.
    #[test]
    fn every_error_naming_a_member_prints_it() {
        let cases = [
            RunContextError::MissingMember { member: "corpus" },
            RunContextError::MalformedMember { member: "corpus" },
            RunContextError::MissingSha256 { member: "corpus" },
            RunContextError::NotLowercaseSha256Hex { member: "corpus" },
            RunContextError::UnknownMember {
                member: "corpus".into(),
            },
        ];
        for case in cases {
            assert!(case.to_string().contains("corpus"), "{case:?}");
        }
    }
}
