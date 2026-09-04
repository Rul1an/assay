//! Deterministic ID generation for Evidence Contract v1.
//!
//! This module provides cryptographic primitives for:
//! - Content-addressed event hashing (content_hash)
//! - Stream identity (run_id:seq)
//! - Deterministic run-root digest (run_root)
//!
//! # Security Invariants
//!
//! 1. `content_hash` MUST NOT include itself in the hash input.
//! 2. Hash inputs use JCS (RFC 8785) canonical JSON.
//! 3. All hashes are SHA-256 with "sha256:" prefix.

use crate::types::EvidenceEvent;
use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::sync::OnceLock;

/// Input struct for content hash computation.
///
/// CRITICAL: This struct defines EXACTLY what goes into the content hash.
///
/// The fields it does NOT cover are not listed here. Every such list written by hand has gone
/// stale — this one omitted `source`, `semantic_digest` and `digest_profile`, and `source` is what
/// names the system that produced the stream, so a reader would have concluded the chain binds
/// something it does not. The complete, enforced inventory lives in
/// `tests/content_hash_field_inventory.rs`, which fails when a field of `EvidenceEvent` is
/// unclassified or classified against observed behaviour. Read that, not a summary of it.
///
/// It INCLUDES:
/// - `specversion` (binds hash to format version)
/// - `type_` (event classification)
/// - `data_content_type` (payload encoding)
/// - `subject` (optional resource identifier)
/// - `payload` (the actual data)
#[derive(Serialize)]
struct ContentHashInput<'a> {
    specversion: &'a str,
    #[serde(rename = "type")]
    type_: &'a str,
    #[serde(rename = "datacontenttype")]
    data_content_type: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")]
    subject: Option<&'a str>,
    #[serde(rename = "data")]
    payload: &'a serde_json::Value,
}

/// One shared constructor for the content-hash preimage.
///
/// Scope introspection and hashing must use this function so the bound-field projection cannot
/// drift from what `compute_content_hash` actually digests.
fn content_hash_input<'a>(event: &'a EvidenceEvent) -> ContentHashInput<'a> {
    ContentHashInput {
        specversion: &event.specversion,
        type_: &event.type_,
        data_content_type: &event.data_content_type,
        subject: event.subject.as_deref(),
        payload: &event.payload,
    }
}

/// Preimage shape used only to project bound field names.
///
/// `subject` is always `Some` here: with `skip_serializing_if = "Option::is_none"`, omitting it
/// would drop `subject` from the serialized key set and falsely green a scope that disagrees with
/// events that carry a subject.
fn content_hash_input_for_scope_projection() -> ContentHashInput<'static> {
    static EMPTY_PAYLOAD: OnceLock<serde_json::Value> = OnceLock::new();
    let payload = EMPTY_PAYLOAD.get_or_init(|| serde_json::json!({}));
    ContentHashInput {
        specversion: "1.0",
        type_: "assay.content-hash.scope",
        data_content_type: "application/json",
        subject: Some("urn:assay:content-hash-scope"),
        payload,
    }
}

fn bound_field_names_from_input(input: &ContentHashInput<'_>) -> Vec<&'static str> {
    let value = serde_json::to_value(input).expect("ContentHashInput serializes");
    let object = value
        .as_object()
        .expect("ContentHashInput serializes to an object");
    let mut names: Vec<&str> = object.keys().map(String::as_str).collect();
    names.sort_unstable();
    names
        .into_iter()
        .map(|name| match name {
            "data" => "data",
            "datacontenttype" => "datacontenttype",
            "specversion" => "specversion",
            "subject" => "subject",
            "type" => "type",
            other => panic!("unexpected ContentHashInput key in scope projection: {other}"),
        })
        .collect()
}

/// Pointers to integrity layers that are *not* the content-hash preimage.
///
/// These are construction facts, not per-bundle verification results. Emitting them does not mean
/// `show` recomputed or verified the archive digest (ADR-044 subject), the events-file digest, or
/// `run_root`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContentHashSeparateIntegrityLayers {
    /// Manifest member digest over `events.ndjson` (bundle file integrity).
    pub events_file_digest: &'static str,
    /// `manifest.run_root` under profile `assay-run-root-v1` (flat SHA-256 over newline-delimited content hashes).
    pub run_root: &'static str,
    /// ADR-044 attestation subject is the archive digest, not the content-hash preimage.
    pub archive_attestation_subject: &'static str,
}

/// Machine-readable content-hash preimage scope.
///
/// ADR-042 non-claims: this object states what the digest binds. It is not producer identity,
/// provenance, completeness, truthfulness, archive verification, tamper intent, a trust score, or
/// a whole-action verdict. Per-bundle verify state stays outside this object (e.g. `verify_mode`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ContentHashScope {
    pub digest_profile: &'static str,
    pub canon: &'static str,
    pub hash: &'static str,
    /// Bound JSON field names in JCS key order, derived from the shared `ContentHashInput` shape.
    pub bound_fields: Vec<&'static str>,
    pub omitted_event_fields: &'static str,
    pub separate_integrity_layers: ContentHashSeparateIntegrityLayers,
}

/// Single source for the additive `content_hash_scope` object.
///
/// Callers (including `assay evidence show --format json`) must embed this value unchanged rather
/// than reconstructing fields from literals.
pub fn content_hash_scope() -> ContentHashScope {
    ContentHashScope {
        digest_profile: "assay-content-hash-v1",
        canon: "jcs-rfc8785",
        hash: "sha256",
        bound_fields: bound_field_names_from_input(&content_hash_input_for_scope_projection()),
        omitted_event_fields: "not_bound_by_this_digest",
        separate_integrity_layers: ContentHashSeparateIntegrityLayers {
            events_file_digest: "manifest.files digest for events.ndjson",
            run_root: "manifest.run_root (assay-run-root-v1)",
            archive_attestation_subject: "ADR-044 archive digest subject (not computed by show)",
        },
    }
}

/// Bound field names in JCS key order from the same input shape `content_hash_scope` uses.
pub fn content_hash_bound_field_names() -> Vec<&'static str> {
    content_hash_scope().bound_fields
}

/// Calculate the Content Hash (sha256 of canonical content).
///
/// This provides cryptographic integrity for "what happened".
/// The hash is computed over a SUBSET of event fields to allow
/// deterministic re-computation and avoid self-reference.
///
/// # Hash Input Fields
///
/// - `specversion`: Binds to format version
/// - `type`: Event classification
/// - `datacontenttype`: Payload encoding
/// - `subject`: Optional resource identifier
/// - `payload` (as `data`): The actual event data
///
/// # Excluded Fields (by design)
///
/// Excluding `time`, stream identity and producer metadata is what makes deterministic re-export
/// possible: the same events, repackaged later by a different producer, keep the same hash.
///
/// The exclusions are NOT enumerated here. This list previously omitted `source`,
/// `semantic_digest` and `digest_profile` while reading as complete. The enforced inventory is
/// `tests/content_hash_field_inventory.rs`; it classifies every field of `EvidenceEvent` against
/// observed behaviour and will not compile once a new field exists.
///
/// # Example
///
/// ```
/// use assay_evidence::crypto::id::compute_content_hash;
/// use assay_evidence::types::EvidenceEvent;
///
/// let event = EvidenceEvent::new(
///     "assay.test",
///     "urn:assay:test",
///     "run_123",
///     0,
///     serde_json::json!({"key": "value"}),
/// );
///
/// let hash = compute_content_hash(&event).unwrap();
/// assert!(hash.starts_with("sha256:"));
/// ```
pub fn compute_content_hash(event: &EvidenceEvent) -> Result<String> {
    assay_canonical::content_id(&content_hash_input(event))
        .context("failed to compute content hash")
}

/// Calculate the Stream Identity ID.
///
/// `run_id` + `seq` provides a unique stream identity per source.
/// CloudEvents require `id` + `source` to be globally unique.
///
/// # Format
///
/// `{run_id}:{seq}` where seq is the decimal sequence number.
///
/// # Example
///
/// ```
/// use assay_evidence::crypto::id::compute_stream_id;
///
/// let id = compute_stream_id("run_abc123", 42);
/// assert_eq!(id, "run_abc123:42");
/// ```
pub fn compute_stream_id(run_id: &str, seq: u64) -> String {
    format!("{}:{}", run_id, seq)
}

/// Calculate the deterministic run-root digest.
///
/// Hashes newline-delimited content-hash strings in event sequence order.
/// This proves the integrity and ordering of the entire event stream.
///
/// # Algorithm
///
/// ```text
/// run_root = sha256( concat( content_hash[0] + "\n" + content_hash[1] + "\n" + ... ) )
/// ```
///
/// # Properties
///
/// - Order-sensitive: reordering events changes the root
/// - Append-only friendly: can compute incrementally
/// - Verifiable: third parties can recompute from events
///
/// # Example
///
/// ```
/// use assay_evidence::crypto::id::compute_run_root;
///
/// let hashes = vec![
///     "sha256:abc123".to_string(),
///     "sha256:def456".to_string(),
/// ];
/// let root = compute_run_root(&hashes);
/// assert!(root.starts_with("sha256:"));
/// ```
pub fn compute_run_root(content_hashes: &[String]) -> String {
    let mut hasher = Sha256::new();
    for hash in content_hashes {
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }
    let hash = hasher.finalize();
    format!("sha256:{}", hex::encode(hash))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::EvidenceEvent;
    use chrono::{TimeZone, Utc};

    /// CRITICAL TEST: Verify content_hash does NOT include itself in computation.
    ///
    /// This test ensures that:
    /// 1. Computing hash on event without content_hash works
    /// 2. Computing hash on event WITH content_hash gives SAME result
    /// 3. The ContentHashInput struct excludes content_hash field
    #[test]
    fn test_content_hash_excludes_self() {
        // Create event without content_hash
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_test",
            0,
            serde_json::json!({"foo": "bar"}),
        );
        event.time = Utc.timestamp_opt(1700000000, 0).unwrap();

        // Compute hash (content_hash is None)
        let hash1 = compute_content_hash(&event).unwrap();

        // Set content_hash to some value
        event.content_hash = Some("sha256:FAKE_HASH_VALUE".to_string());

        // Recompute - should be IDENTICAL because content_hash is excluded
        let hash2 = compute_content_hash(&event).unwrap();

        assert_eq!(
            hash1, hash2,
            "content_hash MUST be excluded from hash computation!\n\
             If this fails, ContentHashInput includes content_hash field."
        );
    }

    /// PR-B: the SOFT semantic_digest / digest_profile are excluded from content_hash, so adding them
    /// is additive — the hard hash (and therefore verification) is unaffected.
    #[test]
    fn test_content_hash_excludes_soft_semantic_digest() {
        let mut event = EvidenceEvent::new(
            "assay.test",
            "urn:assay:test",
            "run_test",
            0,
            serde_json::json!({"foo": "bar"}),
        );
        event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
        let before = compute_content_hash(&event).unwrap();

        event.semantic_digest = Some("sha256:SOFT_VALUE".to_string());
        event.digest_profile = Some("assay.semantic-digest.jcs-rfc8785.v1".to_string());
        let after = compute_content_hash(&event).unwrap();

        assert_eq!(
            before, after,
            "soft semantic_digest/digest_profile MUST be excluded from content_hash (additive, soft)"
        );
    }

    /// Verify that content hash is deterministic (same input = same output)
    #[test]
    fn test_content_hash_determinism() {
        let event1 = create_test_event();
        let event2 = create_test_event();

        let hash1 = compute_content_hash(&event1).unwrap();
        let hash2 = compute_content_hash(&event2).unwrap();

        assert_eq!(hash1, hash2);
    }

    /// Verify that different payloads produce different hashes
    #[test]
    fn test_content_hash_changes_with_payload() {
        let mut event1 = create_test_event();
        let mut event2 = create_test_event();

        event1.payload = serde_json::json!({"value": 1});
        event2.payload = serde_json::json!({"value": 2});

        let hash1 = compute_content_hash(&event1).unwrap();
        let hash2 = compute_content_hash(&event2).unwrap();

        assert_ne!(hash1, hash2);
    }

    /// Verify that different types produce different hashes
    #[test]
    fn test_content_hash_changes_with_type() {
        let mut event1 = create_test_event();
        let mut event2 = create_test_event();

        event1.type_ = "assay.type.one".into();
        event2.type_ = "assay.type.two".into();

        let hash1 = compute_content_hash(&event1).unwrap();
        let hash2 = compute_content_hash(&event2).unwrap();

        assert_ne!(hash1, hash2);
    }

    /// Verify metadata fields DON'T affect content hash
    #[test]
    fn test_content_hash_ignores_metadata() {
        let event1 = create_test_event();
        let mut event2 = create_test_event();

        // Change metadata fields
        event2.run_id = "different_run".into();
        event2.id = "different_run:99".into();
        event2.seq = 99;
        event2.producer = "different_producer".into();
        event2.producer_version = "9.9.9".into();
        event2.git_sha = "zzzzzzz".into();
        event2.time = Utc.timestamp_opt(9999999999, 0).unwrap();
        event2.trace_parent = Some("00-trace-parent".into());
        event2.policy_id = Some("policy_xyz".into());
        event2.contains_pii = true;
        event2.contains_secrets = true;

        let hash1 = compute_content_hash(&event1).unwrap();
        let hash2 = compute_content_hash(&event2).unwrap();

        assert_eq!(
            hash1, hash2,
            "Metadata fields should NOT affect content hash"
        );
    }

    /// Verify stream ID format
    #[test]
    fn test_stream_id_format() {
        assert_eq!(compute_stream_id("run_123", 0), "run_123:0");
        assert_eq!(compute_stream_id("run_abc", 42), "run_abc:42");
        assert_eq!(
            compute_stream_id("complex-run_id.test", 999),
            "complex-run_id.test:999"
        );
    }

    /// Verify run_root is order-sensitive
    #[test]
    fn test_run_root_order_sensitive() {
        let hashes = vec![
            "sha256:aaa".to_string(),
            "sha256:bbb".to_string(),
            "sha256:ccc".to_string(),
        ];

        let reversed = vec![
            "sha256:ccc".to_string(),
            "sha256:bbb".to_string(),
            "sha256:aaa".to_string(),
        ];

        let root1 = compute_run_root(&hashes);
        let root2 = compute_run_root(&reversed);

        assert_ne!(root1, root2, "run_root must be order-sensitive");
    }

    /// Pin the shipped digest: each event content-hash string, then `b"\n"`, in
    /// event sequence order — not delimiter-free concat and not manifest order.
    #[test]
    fn test_run_root_newline_delimited_content_hashes_in_event_order() {
        let hashes = vec!["sha256:aaa".to_string(), "sha256:bbb".to_string()];
        let mut hasher = Sha256::new();
        hasher.update(b"sha256:aaa");
        hasher.update(b"\n");
        hasher.update(b"sha256:bbb");
        hasher.update(b"\n");
        let expected = format!("sha256:{}", hex::encode(hasher.finalize()));
        assert_eq!(compute_run_root(&hashes), expected);

        let mut no_newline = Sha256::new();
        no_newline.update(b"sha256:aaasha256:bbb");
        let delimiter_free = format!("sha256:{}", hex::encode(no_newline.finalize()));
        assert_ne!(
            compute_run_root(&hashes),
            delimiter_free,
            "run_root is not delimiter-free concat of content-hash strings"
        );
    }

    /// Verify empty run_root is valid
    #[test]
    fn test_run_root_empty() {
        let root = compute_run_root(&[]);
        assert!(root.starts_with("sha256:"));
        // Empty input should give sha256 of empty string
        // sha256("") = e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
        assert_eq!(
            root,
            "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    fn create_test_event() -> EvidenceEvent {
        let mut event = EvidenceEvent::new(
            "assay.test.event",
            "urn:assay:test",
            "run_fixed",
            0,
            serde_json::json!({"test": "data"}),
        );
        event.time = Utc.timestamp_opt(1700000000, 0).unwrap();
        event
    }
}
