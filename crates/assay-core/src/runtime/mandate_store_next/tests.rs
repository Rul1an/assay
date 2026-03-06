use super::*;

fn test_metadata() -> MandateMetadata {
    MandateMetadata {
        mandate_id: "sha256:test123".to_string(),
        mandate_kind: "intent".to_string(),
        audience: "org/app".to_string(),
        issuer: "auth.org.com".to_string(),
        expires_at: None,
        single_use: false,
        max_uses: None,
        canonical_digest: "sha256:digest123".to_string(),
        key_id: "sha256:key123".to_string(),
    }
}

fn consume(
    store: &MandateStore,
    mandate_id: &str,
    tool_call_id: &str,
    nonce: Option<&str>,
    audience: &str,
    issuer: &str,
) -> Result<AuthzReceipt, AuthzError> {
    store.consume_mandate(&ConsumeParams {
        mandate_id,
        tool_call_id,
        nonce,
        audience,
        issuer,
        tool_name: "test_tool",
        operation_class: "read",
        source_run_id: None,
    })
}

// === A) Schema/migrations ===

#[test]
fn test_store_bootstraps_schema() {
    let store = MandateStore::memory().unwrap();
    let conn = store.conn.lock().unwrap();

    // Check tables exist
    let tables: Vec<String> = conn
        .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
        .unwrap()
        .query_map([], |row| row.get(0))
        .unwrap()
        .filter_map(|r| r.ok())
        .collect();

    assert!(tables.contains(&"mandates".to_string()));
    assert!(tables.contains(&"mandate_uses".to_string()));
    assert!(tables.contains(&"nonces".to_string()));
}

#[test]
fn test_store_sets_foreign_keys() {
    let store = MandateStore::memory().unwrap();
    let conn = store.conn.lock().unwrap();

    let fk: i32 = conn
        .query_row("PRAGMA foreign_keys", [], |row| row.get(0))
        .unwrap();
    assert_eq!(fk, 1);
}

// === B) Upsert invariants ===

#[test]
fn test_upsert_mandate_inserts_new() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();

    store.upsert_mandate(&meta).unwrap();

    let count = store.get_use_count(&meta.mandate_id).unwrap();
    assert_eq!(count, Some(0));
}

#[test]
fn test_upsert_mandate_is_noop_on_same_content() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();

    store.upsert_mandate(&meta).unwrap();
    store.upsert_mandate(&meta).unwrap(); // Should not error

    let count = store.get_use_count(&meta.mandate_id).unwrap();
    assert_eq!(count, Some(0));
}

#[test]
fn test_upsert_mandate_rejects_conflicting_metadata() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    // Try to upsert with different audience
    let mut conflict = meta.clone();
    conflict.audience = "different/app".to_string();

    let result = store.upsert_mandate(&conflict);
    assert!(matches!(
        result,
        Err(AuthzError::MandateConflict { field, .. }) if field == "audience"
    ));
}

// === C) Consume - idempotency & counts ===

#[test]
fn test_consume_fails_if_mandate_missing() {
    let store = MandateStore::memory().unwrap();

    let result = consume(
        &store,
        "sha256:nonexistent",
        "tc_1",
        None,
        "org/app",
        "auth.org.com",
    );

    assert!(matches!(result, Err(AuthzError::MandateNotFound { .. })));
}

#[test]
fn test_consume_first_time_returns_use_count_1() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    let receipt = consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        None,
        &meta.audience,
        &meta.issuer,
    )
    .unwrap();

    assert_eq!(receipt.use_count, 1);
    assert!(!receipt.use_id.is_empty());
    assert_eq!(receipt.tool_call_id, "tc_1");
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
    assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 1);
}

#[test]
fn test_consume_is_idempotent_for_same_tool_call_id() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    let receipt1 = consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        None,
        &meta.audience,
        &meta.issuer,
    )
    .unwrap();
    let receipt2 = consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        None,
        &meta.audience,
        &meta.issuer,
    )
    .unwrap();

    // Same receipt (idempotent)
    assert_eq!(receipt1.use_id, receipt2.use_id);
    assert_eq!(receipt1.use_count, receipt2.use_count);

    // was_new distinguishes first vs retry
    assert!(receipt1.was_new, "First consume should be was_new=true");
    assert!(!receipt2.was_new, "Retry should be was_new=false");

    // Count didn't increment
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
    assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 1);
}

#[test]
fn test_consume_increments_for_different_tool_call_ids() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    let r1 = consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        None,
        &meta.audience,
        &meta.issuer,
    )
    .unwrap();
    let r2 = consume(
        &store,
        &meta.mandate_id,
        "tc_2",
        None,
        &meta.audience,
        &meta.issuer,
    )
    .unwrap();

    assert_eq!(r1.use_count, 1);
    assert_eq!(r2.use_count, 2);
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(2));
    assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 2);
}

// === D) Constraints - single_use/max_uses ===

#[test]
fn test_single_use_allows_first_then_rejects_second() {
    let store = MandateStore::memory().unwrap();
    let mut meta = test_metadata();
    meta.single_use = true;
    meta.max_uses = Some(1);
    store.upsert_mandate(&meta).unwrap();

    let r1 = consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        None,
        &meta.audience,
        &meta.issuer,
    );
    assert!(r1.is_ok());

    let r2 = consume(
        &store,
        &meta.mandate_id,
        "tc_2",
        None,
        &meta.audience,
        &meta.issuer,
    );
    assert!(matches!(r2, Err(AuthzError::AlreadyUsed)));

    // use_count stayed at 1 (rollback worked)
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
}

#[test]
fn test_max_uses_allows_up_to_n_then_rejects() {
    let store = MandateStore::memory().unwrap();
    let mut meta = test_metadata();
    meta.max_uses = Some(3);
    store.upsert_mandate(&meta).unwrap();

    for i in 1..=3 {
        let r = consume(
            &store,
            &meta.mandate_id,
            &format!("tc_{}", i),
            None,
            &meta.audience,
            &meta.issuer,
        );
        assert!(r.is_ok(), "Call {} should succeed", i);
    }

    let r4 = consume(
        &store,
        &meta.mandate_id,
        "tc_4",
        None,
        &meta.audience,
        &meta.issuer,
    );
    assert!(matches!(
        r4,
        Err(AuthzError::MaxUsesExceeded { max: 3, current: 4 })
    ));

    // use_count stayed at 3
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(3));
}

#[test]
fn test_max_uses_null_is_unlimited() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata(); // max_uses = None
    store.upsert_mandate(&meta).unwrap();

    for i in 1..=20 {
        let r = consume(
            &store,
            &meta.mandate_id,
            &format!("tc_{}", i),
            None,
            &meta.audience,
            &meta.issuer,
        );
        assert!(r.is_ok(), "Call {} should succeed", i);
    }

    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(20));
}

#[test]
fn test_single_use_true_and_max_uses_gt_1_is_invalid() {
    let store = MandateStore::memory().unwrap();
    let mut meta = test_metadata();
    meta.single_use = true;
    meta.max_uses = Some(10); // Invalid: single_use with max > 1

    let result = store.upsert_mandate(&meta);
    assert!(matches!(
        result,
        Err(AuthzError::InvalidConstraints { max_uses: 10 })
    ));
}

// === E) Nonce replay prevention ===

#[test]
fn test_nonce_inserted_on_first_consume() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        Some("nonce_123"),
        &meta.audience,
        &meta.issuer,
    )
    .unwrap();

    assert!(store
        .nonce_exists(&meta.audience, &meta.issuer, "nonce_123")
        .unwrap());
}

#[test]
fn test_nonce_replay_rejected() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    let r1 = consume(
        &store,
        &meta.mandate_id,
        "tc_1",
        Some("nonce_123"),
        &meta.audience,
        &meta.issuer,
    );
    assert!(r1.is_ok());

    // Different tool_call_id, same nonce
    let r2 = consume(
        &store,
        &meta.mandate_id,
        "tc_2",
        Some("nonce_123"),
        &meta.audience,
        &meta.issuer,
    );
    assert!(matches!(r2, Err(AuthzError::NonceReplay { nonce }) if nonce == "nonce_123"));

    // use_count stayed at 1 (rollback worked)
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
}

#[test]
fn test_nonce_scope_is_audience_and_issuer() {
    let store = MandateStore::memory().unwrap();

    // Create two mandates with different audience
    let meta1 = test_metadata();
    let mut meta2 = test_metadata();
    meta2.mandate_id = "sha256:test456".to_string();
    meta2.audience = "different/app".to_string();
    meta2.canonical_digest = "sha256:digest456".to_string();

    store.upsert_mandate(&meta1).unwrap();
    store.upsert_mandate(&meta2).unwrap();

    // Same nonce, different audience should be allowed
    let r1 = consume(
        &store,
        &meta1.mandate_id,
        "tc_1",
        Some("shared_nonce"),
        &meta1.audience,
        &meta1.issuer,
    );
    assert!(r1.is_ok());

    // Same nonce but different audience
    let r2 = consume(
        &store,
        &meta2.mandate_id,
        "tc_2",
        Some("shared_nonce"),
        &meta2.audience,
        &meta2.issuer,
    );
    assert!(
        r2.is_ok(),
        "Same nonce with different audience should be allowed"
    );
}

// === F) Multi-call invariants (serialized via mutex) ===

#[test]
fn test_multicall_produces_monotonic_counts_no_gaps() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    let mut counts = Vec::new();
    for i in 1..=50 {
        let r = consume(
            &store,
            &meta.mandate_id,
            &format!("tc_{}", i),
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();
        counts.push(r.use_count);
    }

    // Verify monotonic: 1, 2, 3, ..., 50
    let expected: Vec<u32> = (1..=50).collect();
    assert_eq!(counts, expected);
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(50));
    assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 50);
}

#[test]
fn test_multicall_idempotent_same_tool_call_id() {
    let store = MandateStore::memory().unwrap();
    let meta = test_metadata();
    store.upsert_mandate(&meta).unwrap();

    let mut receipts = Vec::new();
    for _ in 0..20 {
        let r = consume(
            &store,
            &meta.mandate_id,
            "tc_same",
            None,
            &meta.audience,
            &meta.issuer,
        )
        .unwrap();
        receipts.push(r);
    }

    // All receipts should be identical
    let first = &receipts[0];
    for r in &receipts {
        assert_eq!(r.use_id, first.use_id);
        assert_eq!(r.use_count, first.use_count);
    }

    // Only one actual use
    assert_eq!(store.get_use_count(&meta.mandate_id).unwrap(), Some(1));
    assert_eq!(store.count_uses(&meta.mandate_id).unwrap(), 1);
}

// === H) Revocation API ===

#[test]
fn test_revocation_roundtrip() {
    let store = MandateStore::memory().unwrap();

    let revoked_at = Utc::now();
    let record = RevocationRecord {
        mandate_id: "sha256:revoked123".to_string(),
        revoked_at,
        reason: Some("User requested".to_string()),
        revoked_by: Some("admin@example.com".to_string()),
        source: Some("assay://myorg/myapp".to_string()),
        event_id: Some("evt_revoke_001".to_string()),
    };

    store.upsert_revocation(&record).unwrap();

    let got = store.get_revoked_at(&record.mandate_id).unwrap();
    assert!(got.is_some());
    // Compare within 1 second tolerance (RFC3339 loses sub-second precision)
    let diff = (got.unwrap() - revoked_at).num_seconds().abs();
    assert!(diff <= 1, "revoked_at timestamps differ by {}s", diff);
}

#[test]
fn test_revocation_is_revoked_helper() {
    let store = MandateStore::memory().unwrap();

    assert!(!store.is_revoked("sha256:not_revoked").unwrap());

    store
        .upsert_revocation(&RevocationRecord {
            mandate_id: "sha256:is_revoked".to_string(),
            revoked_at: Utc::now(),
            reason: None,
            revoked_by: None,
            source: None,
            event_id: None,
        })
        .unwrap();

    assert!(store.is_revoked("sha256:is_revoked").unwrap());
}

#[test]
fn test_revocation_upsert_is_idempotent() {
    let store = MandateStore::memory().unwrap();

    let record = RevocationRecord {
        mandate_id: "sha256:idem".to_string(),
        revoked_at: Utc::now(),
        reason: Some("First".to_string()),
        revoked_by: None,
        source: None,
        event_id: None,
    };

    store.upsert_revocation(&record).unwrap();
    store.upsert_revocation(&record).unwrap(); // Should not fail
    store.upsert_revocation(&record).unwrap();

    assert!(store.is_revoked(&record.mandate_id).unwrap());
}

#[test]
fn test_compute_use_id_contract_vector() {
    let use_id = compute_use_id("sha256:m", "tc_1", 2);
    assert_eq!(
        use_id,
        "sha256:333a7fdcb27b62d01a6a56e8c6c57f59782c93f547d4755ee0bcb11fe22fd15c"
    );
}
