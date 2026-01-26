//! Deterministic ID generation for Evidence Contract v1.
//!
//! This module provides cryptographic primitives for:
//! - Content-addressed event hashing (content_hash)
//! - Stream identity (run_id:seq)
//! - Run integrity chain (run_root)
//!
//! # Security Invariants
//!
//! 1. `content_hash` MUST NOT include itself in the hash input.
//! 2. Hash inputs use JCS (RFC 8785) canonical JSON.
//! 3. All hashes are SHA-256 with "sha256:" prefix.

use crate::crypto::jcs;
use crate::types::EvidenceEvent;
use anyhow::Result;
use serde::Serialize;
use sha2::{Digest, Sha256};

/// Input struct for content hash computation.
///
/// CRITICAL: This struct defines EXACTLY what goes into the content hash.
/// It deliberately EXCLUDES:
/// - `content_hash` (would be self-referential)
/// - `id` (derived from run_id + seq)
/// - `time` (allows deterministic re-export)
/// - Trace context (operational metadata)
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
/// - `content_hash`: Would be self-referential
/// - `id`: Derived from run_id + seq
/// - `time`: Allows deterministic re-export
/// - `trace_parent/trace_state`: Operational metadata
/// - `run_id`, `seq`: Stream identity, not content
/// - `producer*`, `git_sha`: Provenance metadata
/// - `policy_id`: Context metadata
/// - `contains_pii/secrets`: Privacy flags
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
    let input = ContentHashInput {
        specversion: &event.specversion,
        type_: &event.type_,
        data_content_type: &event.data_content_type,
        subject: event.subject.as_deref(),
        payload: &event.payload,
    };

    let canonical_bytes = jcs::to_vec(&input)?;
    let hash = Sha256::digest(&canonical_bytes);

    Ok(format!("sha256:{}", hex::encode(hash)))
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

/// Calculate the Run Root (Integrity Chain).
///
/// Creates a hash chain over all content hashes in sequence order.
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
        let mut event1 = create_test_event();
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
