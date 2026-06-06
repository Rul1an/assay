use crate::cache::{PackCache, DEFAULT_TTL_SECS};
use crate::error::RegistryError;
use crate::types::{DsseEnvelope, FetchResult, PackHeaders};
use crate::verify::compute_digest;
use base64::Engine;
use chrono::Utc;
use tempfile::TempDir;
use tokio::fs;

fn create_test_cache() -> (PackCache, TempDir) {
    let temp_dir = TempDir::new().unwrap();
    let cache = PackCache::with_dir(temp_dir.path().join("cache"));
    (cache, temp_dir)
}

fn create_fetch_result(content: &str) -> FetchResult {
    FetchResult {
        content: content.to_string(),
        headers: PackHeaders {
            digest: Some(compute_digest(content)),
            signature: None,
            key_id: None,
            etag: Some("\"abc123\"".to_string()),
            cache_control: Some("max-age=3600".to_string()),
            content_length: Some(content.len() as u64),
        },
        computed_digest: compute_digest(content),
    }
}

#[tokio::test]
async fn test_cache_roundtrip() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = create_fetch_result(content);

    // Put
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Get
    let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
    assert_eq!(entry.content, content);
    assert_eq!(entry.metadata.digest, compute_digest(content));
}

#[tokio::test]
async fn test_cache_miss() {
    let (cache, _temp_dir) = create_test_cache();

    let result = cache.get("nonexistent", "1.0.0").await.unwrap();
    assert!(result.is_none());
}

#[tokio::test]
async fn test_cache_integrity_failure() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = create_fetch_result(content);

    // Put
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Corrupt the cached file
    let pack_path = cache.pack_dir("test-pack", "1.0.0").join("pack.yaml");
    fs::write(&pack_path, "corrupted content").await.unwrap();

    // Get should fail integrity check
    let err = cache.get("test-pack", "1.0.0").await.unwrap_err();
    assert!(matches!(err, RegistryError::DigestMismatch { .. }));
}

#[tokio::test]
async fn test_cache_expiry() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = FetchResult {
        content: content.to_string(),
        headers: PackHeaders {
            digest: Some(compute_digest(content)),
            signature: None,
            key_id: None,
            etag: None,
            cache_control: Some("max-age=0".to_string()), // Expire immediately
            content_length: None,
        },
        computed_digest: compute_digest(content),
    };

    // Put
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Wait a moment
    tokio::time::sleep(std::time::Duration::from_millis(10)).await;

    // Get should return None (expired)
    let entry = cache.get("test-pack", "1.0.0").await.unwrap();
    assert!(entry.is_none());
}

#[tokio::test]
async fn test_cache_evict() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = create_fetch_result(content);

    // Put
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();
    assert!(cache.is_cached("test-pack", "1.0.0").await);

    // Evict
    cache.evict("test-pack", "1.0.0").await.unwrap();
    assert!(!cache.is_cached("test-pack", "1.0.0").await);
}

#[tokio::test]
async fn test_cache_clear() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = create_fetch_result(content);

    // Put multiple packs
    cache.put("pack1", "1.0.0", &result, None).await.unwrap();
    cache.put("pack2", "1.0.0", &result, None).await.unwrap();

    // Clear
    cache.clear().await.unwrap();

    // Both should be gone
    assert!(!cache.is_cached("pack1", "1.0.0").await);
    assert!(!cache.is_cached("pack2", "1.0.0").await);
}

#[tokio::test]
async fn test_cache_list() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = create_fetch_result(content);

    // Put multiple packs
    cache.put("pack1", "1.0.0", &result, None).await.unwrap();
    cache.put("pack1", "2.0.0", &result, None).await.unwrap();
    cache.put("pack2", "1.0.0", &result, None).await.unwrap();

    // List
    let entries = cache.list().await.unwrap();
    assert_eq!(entries.len(), 3);
}

#[tokio::test]
async fn test_get_etag() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";
    let result = create_fetch_result(content);

    // Put
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Get ETag
    let etag = cache.get_etag("test-pack", "1.0.0").await;
    assert_eq!(etag, Some("\"abc123\"".to_string()));
}

#[tokio::test]
async fn test_parse_cache_control() {
    let headers = PackHeaders {
        digest: None,
        signature: None,
        key_id: None,
        etag: None,
        cache_control: Some("max-age=7200, public".to_string()),
        content_length: None,
    };

    let expires = super::policy::parse_cache_control_expiry_impl(&headers, DEFAULT_TTL_SECS);
    let now = Utc::now();

    // Should be approximately 2 hours in the future
    let diff = expires - now;
    assert!(diff.num_seconds() >= 7190 && diff.num_seconds() <= 7210);
}

#[tokio::test]
async fn test_default_ttl() {
    let headers = PackHeaders {
        digest: None,
        signature: None,
        key_id: None,
        etag: None,
        cache_control: None, // No Cache-Control
        content_length: None,
    };

    let expires = super::policy::parse_cache_control_expiry_impl(&headers, DEFAULT_TTL_SECS);
    let now = Utc::now();

    // Should be approximately 24 hours in the future
    let diff = expires - now;
    assert!(diff.num_hours() >= 23 && diff.num_hours() <= 25);
}

#[tokio::test]
async fn test_cache_with_signature() {
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: 1.0.0";

    // Create a mock DSSE envelope
    let envelope = DsseEnvelope {
        payload_type: "application/vnd.assay.pack+yaml;v=1".to_string(),
        payload: base64::engine::general_purpose::STANDARD.encode(content),
        signatures: vec![],
    };
    let envelope_json = serde_json::to_vec(&envelope).unwrap();
    let envelope_b64 = base64::engine::general_purpose::STANDARD.encode(&envelope_json);

    let result = FetchResult {
        content: content.to_string(),
        headers: PackHeaders {
            digest: Some(compute_digest(content)),
            signature: Some(envelope_b64),
            key_id: Some("sha256:test-key".to_string()),
            etag: None,
            cache_control: Some("max-age=3600".to_string()),
            content_length: None,
        },
        computed_digest: compute_digest(content),
    };

    // Put
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Get
    let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
    assert!(entry.signature.is_some());
    assert_eq!(
        entry.signature.unwrap().payload_type,
        "application/vnd.assay.pack+yaml;v=1"
    );
}

// ==================== Cache Robustness Tests (SPEC §7.2) ====================

#[tokio::test]
async fn test_pack_yaml_corrupt_evict_refetch() {
    // SPEC §7.2: Corrupted cache entry should be detected and evictable
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: \"1.0.0\"";
    let result = create_fetch_result(content);

    // Put valid content
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Verify it works
    let entry = cache.get("test-pack", "1.0.0").await.unwrap();
    assert!(entry.is_some());

    // Corrupt the cached file
    let pack_path = cache.pack_dir("test-pack", "1.0.0").join("pack.yaml");
    fs::write(&pack_path, "corrupted: content\nmalicious: true")
        .await
        .unwrap();

    // Get should fail with DigestMismatch
    let err = cache.get("test-pack", "1.0.0").await.unwrap_err();
    assert!(
        matches!(err, RegistryError::DigestMismatch { .. }),
        "Should detect corruption: {:?}",
        err
    );

    // Evict the corrupted entry
    cache.evict("test-pack", "1.0.0").await.unwrap();

    // Now cache should be empty
    let entry = cache.get("test-pack", "1.0.0").await.unwrap();
    assert!(entry.is_none(), "Cache should be empty after evict");
}

#[tokio::test]
async fn test_signature_json_corrupt_handling() {
    // SPEC §7.2: Corrupted signature.json should not crash, signature becomes None
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: \"1.0.0\"";

    // Create with valid signature
    let envelope = DsseEnvelope {
        payload_type: "application/vnd.assay.pack+yaml;v=1".to_string(),
        payload: base64::engine::general_purpose::STANDARD.encode(content),
        signatures: vec![],
    };
    let envelope_json = serde_json::to_vec(&envelope).unwrap();
    let envelope_b64 = base64::engine::general_purpose::STANDARD.encode(&envelope_json);

    let result = FetchResult {
        content: content.to_string(),
        headers: PackHeaders {
            digest: Some(compute_digest(content)),
            signature: Some(envelope_b64),
            key_id: Some("sha256:test-key".to_string()),
            etag: None,
            cache_control: Some("max-age=3600".to_string()),
            content_length: None,
        },
        computed_digest: compute_digest(content),
    };

    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Verify signature exists
    let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
    assert!(entry.signature.is_some());

    // Corrupt the signature file
    let sig_path = cache.pack_dir("test-pack", "1.0.0").join("signature.json");
    fs::write(&sig_path, "this is not valid json{{{")
        .await
        .unwrap();

    // Get should still work, but signature is None (graceful degradation)
    let entry = cache.get("test-pack", "1.0.0").await.unwrap().unwrap();
    assert!(
        entry.signature.is_none(),
        "Corrupt signature should be None, not error"
    );
    // Content should still be valid
    assert_eq!(entry.content, content);
}

#[tokio::test]
async fn test_metadata_json_corrupt_handling() {
    // SPEC §7.2: Corrupted metadata.json should return cache miss
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: \"1.0.0\"";
    let result = create_fetch_result(content);

    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    // Corrupt the metadata file
    let meta_path = cache.pack_dir("test-pack", "1.0.0").join("metadata.json");
    fs::write(&meta_path, "invalid json content").await.unwrap();

    // Get should fail with cache error (not crash)
    let result = cache.get("test-pack", "1.0.0").await;
    assert!(
        matches!(result, Err(RegistryError::Cache { .. })),
        "Should return cache error for corrupt metadata: {:?}",
        result
    );
}

#[tokio::test]
async fn test_atomic_write_prevents_partial_cache() {
    // SPEC §7.2: Atomic writes prevent partial/corrupt cache entries
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: \"1.0.0\"";
    let result = create_fetch_result(content);

    // After put, no .tmp files should exist
    cache
        .put("test-pack", "1.0.0", &result, None)
        .await
        .unwrap();

    let pack_dir = cache.pack_dir("test-pack", "1.0.0");

    // Check no temp files remain
    let mut entries = fs::read_dir(&pack_dir).await.unwrap();
    while let Some(entry) = entries.next_entry().await.unwrap() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        assert!(
            !name_str.ends_with(".tmp"),
            "Temp file should not remain: {}",
            name_str
        );
    }
}

#[tokio::test]
async fn test_cache_registry_url_tracking() {
    // SPEC §7.1: Cache should track which registry pack came from
    let (cache, _temp_dir) = create_test_cache();
    let content = "name: test\nversion: \"1.0.0\"";
    let result = create_fetch_result(content);

    cache
        .put(
            "test-pack",
            "1.0.0",
            &result,
            Some("https://registry.example.com/v1"),
        )
        .await
        .unwrap();

    let meta = cache.get_metadata("test-pack", "1.0.0").await.unwrap();
    assert_eq!(
        meta.registry_url,
        Some("https://registry.example.com/v1".to_string())
    );
}
