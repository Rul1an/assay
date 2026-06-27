//! EXPERIMENTAL (unstable, may change): the observed-input side of the tool-decision truth-layer
//! carrier. It provides a keyed, domain-separated `args_digest` (raw arguments are never stored), an
//! `observed_input_digest` over the stable `{tool_name, args_digest, order}` triple, and a 3-zone
//! carrier record whose decision identity is the `(observed_input_digest, declared_policy_digest)` pair.
//! The declared side is [`super::policy::McpPolicy::declared_constraint_digest_experimental`].
//!
//! It also provides a deterministic verdict gate over every axis the declared digest binds (tool name,
//! args schema, identity, classes, approval, scope, redaction), folded with the lattice
//! `invalid > mismatch > incomplete > match`, plus a run-level aggregate over an ordered set of decisions.
//! A declared constraint the gate cannot yet evaluate resolves to `incomplete`, so `match` never silently
//! means "the subset we checked matched". Not a stability guarantee: the schema, field names, and digests
//! may change until this is promoted out of experimental.

mod digest;
mod pack;
mod verdict;

pub use digest::{args_digest, build_record, observed_input_digest};
pub use pack::{
    carrier_content_digest, decision_identity_digest, evidence_ref, pack_recipe_row,
    verify_recipe_row, RECIPE,
};
pub use verdict::{build_classified_record, decision_verdict, run_verdict, DecisionEvidence};

/// Experimental schema id for the carrier record.
pub const SCHEMA: &str = "assay.tool_decision_truth.v0";

/// Whether `s` is a well-formed `sha256:<64 lowercase hex>` digest.
fn is_sha256_digest(s: &str) -> bool {
    match s.strip_prefix("sha256:") {
        Some(hex) => hex.len() == 64 && hex.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')),
        None => false,
    }
}
