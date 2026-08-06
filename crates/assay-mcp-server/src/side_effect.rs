//! Side-effect receipt (Eb): the honesty ladder and the binding verifier.
//!
//! Implements `docs/reference/side-effect-receipt.md`, whose fixtures under
//! `tests/fixtures/side_effect/` are the acceptance criteria. Ea froze the spec, the ladder and the
//! vectors and proved the binding math; this is the producer and the verifier it named as later
//! slices.
//!
//! # The one rule
//!
//! > Verified never means "Assay queried the provider." It means an independently produced audit
//! > record, whose binding Assay recomputes from committed bytes, matches the observed call.
//!
//! Nothing here opens a socket, and nothing here takes a provider credential. A verifier that
//! re-fetches state is an actor, and an actor holding read credentials for every provider it watches
//! is the confused deputy the MCP threat model warns about. The identity worth having is the one that
//! reproduces a verdict from committed bytes.
//!
//! # What a level is worth
//!
//! `asserted` never auto-promotes. A higher level is reached only through the evidence its own row
//! names, and the record always says which. A record that fails a check leaves the level at
//! `asserted` and is **reported** — [`AuditBinding`] carries the reason — because a silently dropped
//! audit record is indistinguishable from one that was never imported.
//!
//! # What a recomputed binding is not
//!
//! It is not a re-derived decision. Agents are non-deterministic, so replaying a past decision today
//! can legitimately yield a different answer, and the 2026 audit-trail literature is explicit that
//! recomputation is no substitute for capture. What is recomputed here is a **digest over committed
//! bytes**. Conflating the two would claim far more than this mechanism supports.

use assay_core::mcp::jcs;
use serde::{Deserialize, Serialize};
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

/// Schema of the imported, independently produced provider audit record.
pub const PROVIDER_AUDIT_RECORD_SCHEMA: &str = "assay.provider_audit_record.v0";

/// Fields projected from a decision's `action.target` into the binding subject.
///
/// The subject names the action, never the observation: `provider` is already implied by the record's
/// own `provider` field and `read_only` is a property of the request rather than of the effect, so
/// neither belongs in a digest that must match an audit entry produced by a system that never saw our
/// request. Widening this list changes every binding and is a schema break.
const TARGET_SUBJECT_FIELDS: &[&str] = &["owner", "repo", "key_title_hash"];

/// How far past "the tool said success" the evidence actually reaches.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SideEffectLevel {
    /// The tool returned success. The provider's claim, not proof.
    Asserted,
    /// A later observed read call in the same run returned consistent state. Sequence evidence
    /// inside the run, never external verification.
    ObservedConfirmed,
    /// An imported, independently produced audit record binds to *this* call and the binding
    /// recomputed.
    Verified,
}

/// What produced a level above `asserted`. Absent for `asserted`, so a level can never be read
/// without the evidence class that earned it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationSource {
    ObservedReadFollowup,
    ProviderAuditImport,
}

/// The `response.side_effect` block.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SideEffect {
    /// Mirrors the legacy `side_effect_asserted` for compatibility.
    pub asserted: bool,
    pub level: SideEffectLevel,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_source: Option<VerificationSource>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verification_subject_digest: Option<String>,
}

impl SideEffect {
    /// The only constructor a producer may use. Everything starts here and is promoted by evidence.
    #[must_use]
    pub fn asserted(asserted: bool) -> Self {
        Self {
            asserted,
            level: SideEffectLevel::Asserted,
            verification_source: None,
            verification_subject_digest: None,
        }
    }

    /// The compat boolean. True for `verified` only: `observed_confirmed` is in-run sequence
    /// evidence and must not read as external verification.
    #[must_use]
    pub fn verified_flag(&self) -> bool {
        self.level == SideEffectLevel::Verified
    }

    /// Promote on a confirmed binding. Private on purpose — promotion is only reachable through
    /// [`promote_with_audit_record`] or [`promote_with_observed_read`], both of which require the
    /// evidence first.
    fn promoted(
        mut self,
        level: SideEffectLevel,
        source: VerificationSource,
        digest: String,
    ) -> Self {
        self.level = level;
        self.verification_source = Some(source);
        self.verification_subject_digest = Some(digest);
        self
    }
}

/// Outcome of checking an imported audit record against an observed call.
///
/// A rejection is a value, not a `None`: the caller has to carry the reason so a record that failed
/// is reported rather than silently indistinguishable from one that was never imported.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case")]
pub enum AuditBinding {
    /// Both spec checks hold: the record is internally consistent AND binds to this call.
    Bound { subject_digest: String },
    /// The record's own `binding_digest` does not equal the digest of its own `subject`.
    RecordInconsistent {
        recomputed: String,
        declared: String,
    },
    /// The record is internally consistent but describes a different call.
    BindsDifferentCall {
        record_digest: String,
        call_digest: String,
    },
    /// The record or the call could not be projected into a subject at all.
    NotProjectable { reason: &'static str },
}

impl AuditBinding {
    #[must_use]
    pub fn is_bound(&self) -> bool {
        matches!(self, Self::Bound { .. })
    }
}

/// `sha256(jcs(subject))`, the binding digest for a canonical subject.
///
/// # Errors
/// Returns `None` when the subject cannot be canonicalized.
#[must_use]
pub fn binding_digest(subject: &Value) -> Option<String> {
    let bytes = jcs::to_vec(subject).ok()?;
    Some(format!("sha256:{}", hex::encode(Sha256::digest(&bytes))))
}

/// Project an observed decision's `action` into the canonical binding subject.
///
/// `{action_class, verb, target}` where `action_class` is the decision's `resource_type` and `target`
/// is narrowed to [`TARGET_SUBJECT_FIELDS`]. Returns `None` when the decision is unclassified: a null
/// verb or resource_type means there is nothing to bind, and inventing a subject from an
/// unclassified action would mint bindings that match nothing on purpose.
#[must_use]
pub fn subject_from_action(action: &Value) -> Option<Value> {
    let action_class = action.get("resource_type")?.as_str()?;
    let verb = action.get("verb")?.as_str()?;
    let target = action.get("target")?.as_object()?;

    let mut projected = Map::new();
    for field in TARGET_SUBJECT_FIELDS {
        if let Some(value) = target.get(*field) {
            projected.insert((*field).to_string(), value.clone());
        }
    }
    if projected.is_empty() {
        return None;
    }

    Some(json!({
        "action_class": action_class,
        "verb": verb,
        "target": Value::Object(projected),
    }))
}

/// Check an imported audit record against an observed decision's action.
///
/// Both checks from the spec, in this order, because the first failing check is the informative one:
///
/// 1. the record recomputes to its own `binding_digest` (internally consistent);
/// 2. that digest equals the digest of the observed call's action projection (it binds to *this*
///    call, not merely to some call of the same shape).
#[must_use]
pub fn check_audit_record(record: &Value, action: &Value) -> AuditBinding {
    let Some(subject) = record.get("subject") else {
        return AuditBinding::NotProjectable {
            reason: "audit record has no subject",
        };
    };
    let Some(declared) = record.get("binding_digest").and_then(Value::as_str) else {
        return AuditBinding::NotProjectable {
            reason: "audit record has no binding_digest",
        };
    };
    let Some(recomputed) = binding_digest(subject) else {
        return AuditBinding::NotProjectable {
            reason: "audit record subject is not canonicalizable",
        };
    };
    if recomputed != declared {
        return AuditBinding::RecordInconsistent {
            recomputed,
            declared: declared.to_string(),
        };
    }

    let Some(call_subject) = subject_from_action(action) else {
        return AuditBinding::NotProjectable {
            reason: "observed action is unclassified",
        };
    };
    let Some(call_digest) = binding_digest(&call_subject) else {
        return AuditBinding::NotProjectable {
            reason: "observed action is not canonicalizable",
        };
    };
    if call_digest != recomputed {
        return AuditBinding::BindsDifferentCall {
            record_digest: recomputed,
            call_digest,
        };
    }

    AuditBinding::Bound {
        subject_digest: recomputed,
    }
}

/// Promote to `verified` if and only if an imported record binds. Returns the outcome alongside the
/// side effect so a caller cannot promote without also holding the reason it did or did not.
#[must_use]
pub fn promote_with_audit_record(
    side_effect: SideEffect,
    record: &Value,
    action: &Value,
) -> (SideEffect, AuditBinding) {
    // An unasserted side effect has nothing to verify: there is no claimed effect to bind to.
    if !side_effect.asserted {
        return (
            side_effect,
            AuditBinding::NotProjectable {
                reason: "side effect was not asserted",
            },
        );
    }
    let binding = check_audit_record(record, action);
    match &binding {
        AuditBinding::Bound { subject_digest } => (
            side_effect.promoted(
                SideEffectLevel::Verified,
                VerificationSource::ProviderAuditImport,
                subject_digest.clone(),
            ),
            binding,
        ),
        _ => (side_effect, binding),
    }
}

/// Promote to `observed_confirmed` on a later read in the same run whose target projects to the same
/// subject as the write.
///
/// Pure sequence reasoning over calls already observed: no new credential, no new capability, and no
/// promotion past `observed_confirmed`, because agreeing with yourself later in the same run is not
/// external verification.
#[must_use]
pub fn promote_with_observed_read(
    side_effect: SideEffect,
    write_action: &Value,
    read_action: &Value,
) -> SideEffect {
    if !side_effect.asserted || side_effect.level != SideEffectLevel::Asserted {
        return side_effect;
    }
    let (Some(write), Some(read)) = (
        subject_from_action(write_action),
        subject_from_action(read_action),
    ) else {
        return side_effect;
    };
    // Same resource, not the same verb: a read confirming a write is a different action on one
    // target, so compare the bound target and class rather than the whole subject.
    if write["action_class"] != read["action_class"] || write["target"] != read["target"] {
        return side_effect;
    }
    let Some(digest) = binding_digest(&write) else {
        return side_effect;
    };
    side_effect.promoted(
        SideEffectLevel::ObservedConfirmed,
        VerificationSource::ObservedReadFollowup,
        digest,
    )
}
