//! Deterministic canonicalization for Assay semantic digests.
//!
//! Layers, smallest to largest:
//!
//! 1. [`parse_strict`] — parse raw JSON into a [`serde_json::Value`] while **rejecting duplicate
//!    object keys**. The stdlib path (`serde_json::from_str::<Value>`) silently collapses duplicates
//!    (last wins), which would erase an ambiguity before canonicalization can reject it. This is the
//!    fail-closed entry point for untrusted JSON at the canonical boundary.
//! 2. [`jcs`] — RFC 8785 (JSON Canonicalization Scheme) bytes, via the pinned `serde_jcs`. Object
//!    keys are sorted; arrays are left exactly as emitted. Byte-for-byte the canonical form
//!    `assay-evidence` already uses for its content and mandate IDs.
//! 3. [`content_id`] — `"sha256:" + hex(sha256(jcs_bytes))`, the content-addressed id used across
//!    Assay evidence. Hashes the value *as given*; arrays are not reordered.
//! 4. [`set_paths::normalize_sets`] — schema-aware normalization: sort + dedupe ONLY the array
//!    fields a schema registers as semantic sets, before canonicalization. The one place array order
//!    is changed, and only for registered paths.
//! 5. [`semantic_digest`] — the product-facing digest: normalize the registered set-paths, bind the
//!    [`PROFILE`] into the preimage, then content-address. Binding the profile means a digest under
//!    one profile never collides with another, and a consumer can reject a profile it does not
//!    implement ([`ensure_supported_profile`]) instead of silently recomputing under newer rules.
//!
//! ## Two digests, on purpose
//!
//! - [`content_id`] alone is an **as-received** digest: "are these the same bytes I saw?". Callers
//!   doing forensic correlation use it directly and must NOT set-normalize — sorting a set-valued
//!   array would make genuinely different received payloads collide.
//! - [`semantic_digest`] is a **semantic-equivalence** digest: two records that differ only in the
//!   order or duplication of a registered set, under the same profile, collapse to one id.
//!
//! Which fields are sets is a per-schema decision held in the semantic-digest contract registry,
//! supplied by the caller; this crate is the mechanism, not the registry.

pub mod digest;
pub mod jcs;
pub mod parse;
pub mod profile;
pub mod set_paths;

pub use digest::content_id;
pub use parse::parse_strict;
pub use profile::{ensure_supported_profile, semantic_digest, PROFILE};

/// An error from canonicalizing, parsing, or profiling a value.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    /// The value could not be serialized to canonical (RFC 8785) JSON.
    #[error("canonicalization failed: {0}")]
    Canonicalize(String),
    /// Raw JSON could not be parsed under the strict (duplicate-key-rejecting) rules.
    #[error("strict JSON parse failed: {0}")]
    Parse(String),
    /// A record was produced under a `canonicalization_profile` this build does not implement; a
    /// consumer must fail closed rather than recompute it under the current rules.
    #[error("unknown canonicalization profile: {0}")]
    UnknownProfile(String),
    /// A registered set-path could not be normalized.
    #[error(transparent)]
    SetPath(#[from] set_paths::SetPathError),
}
