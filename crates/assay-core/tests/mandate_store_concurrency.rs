//! Multi-connection concurrency tests for MandateStore.
//!
//! These tests use two separate connections to the same file-backed DB
//! to verify that SQLite constraints work correctly under real concurrency.

use assay_core::runtime::{AuthzError, ConsumeParams, MandateMetadata, MandateStore};
use std::sync::Arc;
use std::thread;
use tempfile::NamedTempFile;

fn test_metadata(id_suffix: &str) -> MandateMetadata {
    MandateMetadata {
        mandate_id: format!("sha256:test{}", id_suffix),
        mandate_kind: "intent".to_string(),
        audience: "org/app".to_string(),
        issuer: "auth.org.com".to_string(),
        expires_at: None,
        single_use: false,
        max_uses: None,
        canonical_digest: format!("sha256:digest{}", id_suffix),
        key_id: "sha256:key123".to_string(),
    }
}

/// Test: Two connections racing to use same nonce → exactly one succeeds.
///
/// This tests real SQLite constraint behavior, not just mutex serialization.
#[test]
fn test_two_connections_nonce_replay_one_succeeds() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path();

    // Setup: Create mandate via first connection
    let store1 = MandateStore::open(path).unwrap();
    let meta = test_metadata("shared");
    store1.upsert_mandate(&meta).unwrap();

    // Create second connection to same DB
    let store2 = MandateStore::open(path).unwrap();

    // Race: Both try to consume with same nonce
    let store1 = Arc::new(store1);
    let store2 = Arc::new(store2);
    let meta_clone = meta.clone();

    let s1 = store1.clone();
    let m1 = meta_clone.clone();
    let h1 = thread::spawn(move || {
        s1.consume_mandate(&ConsumeParams {
            mandate_id: &m1.mandate_id,
            tool_call_id: "tc_conn1",
            nonce: Some("shared_nonce"),
            audience: &m1.audience,
            issuer: &m1.issuer,
            tool_name: "tool",
            operation_class: "commit",
            source_run_id: None,
        })
    });

    let s2 = store2.clone();
    let m2 = meta_clone;
    let h2 = thread::spawn(move || {
        s2.consume_mandate(&ConsumeParams {
            mandate_id: &m2.mandate_id,
            tool_call_id: "tc_conn2",
            nonce: Some("shared_nonce"),
            audience: &m2.audience,
            issuer: &m2.issuer,
            tool_name: "tool",
            operation_class: "commit",
            source_run_id: None,
        })
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Exactly one should succeed, one should fail with NonceReplay
    let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let replays = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(AuthzError::NonceReplay { .. })))
        .count();

    assert_eq!(successes, 1, "Exactly one connection should succeed");
    assert_eq!(replays, 1, "Exactly one connection should get NonceReplay");

    // Verify final state
    assert_eq!(store1.get_use_count(&meta.mandate_id).unwrap(), Some(1));
}

/// Test: Two connections racing with same tool_call_id → idempotent.
///
/// Both should get the same receipt (or one gets it, other is blocked until first commits).
#[test]
fn test_two_connections_same_tool_call_id_idempotent() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path();

    // Setup
    let store1 = MandateStore::open(path).unwrap();
    let meta = test_metadata("idem");
    store1.upsert_mandate(&meta).unwrap();

    let store2 = MandateStore::open(path).unwrap();

    let store1 = Arc::new(store1);
    let store2 = Arc::new(store2);
    let meta_clone = meta.clone();

    let s1 = store1.clone();
    let m1 = meta_clone.clone();
    let h1 = thread::spawn(move || {
        s1.consume_mandate(&ConsumeParams {
            mandate_id: &m1.mandate_id,
            tool_call_id: "tc_shared", // Same tool_call_id
            nonce: None,
            audience: &m1.audience,
            issuer: &m1.issuer,
            tool_name: "tool",
            operation_class: "read",
            source_run_id: None,
        })
    });

    let s2 = store2.clone();
    let m2 = meta_clone;
    let h2 = thread::spawn(move || {
        s2.consume_mandate(&ConsumeParams {
            mandate_id: &m2.mandate_id,
            tool_call_id: "tc_shared", // Same tool_call_id
            nonce: None,
            audience: &m2.audience,
            issuer: &m2.issuer,
            tool_name: "tool",
            operation_class: "read",
            source_run_id: None,
        })
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Both should succeed
    assert!(r1.is_ok(), "First connection should succeed");
    assert!(r2.is_ok(), "Second connection should succeed (idempotent)");

    // Both should have same use_id and use_count
    let receipt1 = r1.unwrap();
    let receipt2 = r2.unwrap();
    assert_eq!(
        receipt1.use_id, receipt2.use_id,
        "Same tool_call_id → same use_id"
    );
    assert_eq!(
        receipt1.use_count, receipt2.use_count,
        "Same tool_call_id → same use_count"
    );

    // Only one actual use in DB
    assert_eq!(store1.get_use_count(&meta.mandate_id).unwrap(), Some(1));
    assert_eq!(store1.count_uses(&meta.mandate_id).unwrap(), 1);
}

/// Test: Two connections racing on single_use mandate → exactly one succeeds.
#[test]
fn test_two_connections_single_use_one_succeeds() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path();

    // Setup: Single-use mandate
    let store1 = MandateStore::open(path).unwrap();
    let mut meta = test_metadata("single");
    meta.single_use = true;
    meta.max_uses = Some(1);
    store1.upsert_mandate(&meta).unwrap();

    let store2 = MandateStore::open(path).unwrap();

    let store1 = Arc::new(store1);
    let store2 = Arc::new(store2);
    let meta_clone = meta.clone();

    let s1 = store1.clone();
    let m1 = meta_clone.clone();
    let h1 = thread::spawn(move || {
        s1.consume_mandate(&ConsumeParams {
            mandate_id: &m1.mandate_id,
            tool_call_id: "tc_conn1", // Different tool_call_ids
            nonce: None,
            audience: &m1.audience,
            issuer: &m1.issuer,
            tool_name: "tool",
            operation_class: "read",
            source_run_id: None,
        })
    });

    let s2 = store2.clone();
    let m2 = meta_clone;
    let h2 = thread::spawn(move || {
        s2.consume_mandate(&ConsumeParams {
            mandate_id: &m2.mandate_id,
            tool_call_id: "tc_conn2", // Different tool_call_ids
            nonce: None,
            audience: &m2.audience,
            issuer: &m2.issuer,
            tool_name: "tool",
            operation_class: "read",
            source_run_id: None,
        })
    });

    let r1 = h1.join().unwrap();
    let r2 = h2.join().unwrap();

    // Exactly one should succeed, one should fail with AlreadyUsed
    let successes = [&r1, &r2].iter().filter(|r| r.is_ok()).count();
    let already_used = [&r1, &r2]
        .iter()
        .filter(|r| matches!(r, Err(AuthzError::AlreadyUsed)))
        .count();

    assert_eq!(successes, 1, "Exactly one connection should succeed");
    assert_eq!(
        already_used, 1,
        "Exactly one connection should get AlreadyUsed"
    );

    // Final state: use_count = 1
    assert_eq!(store1.get_use_count(&meta.mandate_id).unwrap(), Some(1));
}

/// Test: Many connections racing → counts are monotonic with no gaps.
#[test]
fn test_many_connections_monotonic_counts() {
    let tmp = NamedTempFile::new().unwrap();
    let path = tmp.path();

    // Setup
    let store_setup = MandateStore::open(path).unwrap();
    let meta = test_metadata("many");
    store_setup.upsert_mandate(&meta).unwrap();
    drop(store_setup);

    // Spawn 10 threads, each with own connection
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = path.to_path_buf();
            let meta = meta.clone();
            thread::spawn(move || {
                let store = MandateStore::open(&path).unwrap();
                store.consume_mandate(&ConsumeParams {
                    mandate_id: &meta.mandate_id,
                    tool_call_id: &format!("tc_thread_{}", i),
                    nonce: None,
                    audience: &meta.audience,
                    issuer: &meta.issuer,
                    tool_name: "tool",
                    operation_class: "read",
                    source_run_id: None,
                })
            })
        })
        .collect();

    let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();

    // All should succeed
    for (i, r) in results.iter().enumerate() {
        assert!(r.is_ok(), "Thread {} should succeed: {:?}", i, r);
    }

    // Collect use_counts
    let mut counts: Vec<u32> = results
        .iter()
        .map(|r| r.as_ref().unwrap().use_count)
        .collect();
    counts.sort();

    // Should be 1..=10 with no gaps
    let expected: Vec<u32> = (1..=10).collect();
    assert_eq!(counts, expected, "Counts should be monotonic 1..10");

    // Verify final state
    let store_final = MandateStore::open(path).unwrap();
    assert_eq!(
        store_final.get_use_count(&meta.mandate_id).unwrap(),
        Some(10)
    );
    assert_eq!(store_final.count_uses(&meta.mandate_id).unwrap(), 10);
}
