//! Integration tests for RegistryClient.
//!
//! Uses wiremock for HTTP mocking. Tests cover fetch_pack, fetch_signature,
//! fetch_pack_with_signature, status mapping (304/404/404-sig/410/429/5xx), and retry behavior.

use std::time::Duration;

use assay_registry::{
    compute_digest, RegistryClient, RegistryConfig, RegistryError, REGISTRY_USER_AGENT,
};
use wiremock::matchers::{header, method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

async fn create_test_client(mock_server: &MockServer) -> RegistryClient {
    let config = RegistryConfig::default()
        .with_url(mock_server.uri())
        .with_token("test-token");
    RegistryClient::new(config).expect("failed to create client")
}

#[tokio::test]
async fn test_fetch_pack_success() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: test-pack\nversion: \"1.0.0\"\nrules: []";
    let expected_digest = compute_digest(pack_yaml);

    Mock::given(method("GET"))
        .and(path("/packs/test-pack/1.0.0"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("x-pack-digest", expected_digest.as_str())
                .insert_header("etag", "\"abc123\""),
        )
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_pack("test-pack", "1.0.0", None)
        .await
        .expect("fetch failed");

    let fetch = result.expect("expected Some");
    assert_eq!(fetch.content, pack_yaml);
    assert_eq!(fetch.computed_digest, expected_digest);
    assert_eq!(fetch.headers.etag, Some("\"abc123\"".to_string()));
}

#[tokio::test]
async fn test_fetch_pack_304_not_modified() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/test-pack/1.0.0"))
        .and(header("if-none-match", "\"abc123\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_pack("test-pack", "1.0.0", Some("\"abc123\""))
        .await
        .expect("fetch failed");

    assert!(result.is_none(), "expected None for 304");
}

#[tokio::test]
async fn test_fetch_pack_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/nonexistent/1.0.0"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("nonexistent", "1.0.0", None).await;

    assert!(matches!(result, Err(RegistryError::NotFound { .. })));
}

#[tokio::test]
async fn test_fetch_pack_unauthorized() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/private-pack/1.0.0"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("private-pack", "1.0.0", None).await;

    assert!(matches!(result, Err(RegistryError::Unauthorized { .. })));
}

#[tokio::test]
async fn test_fetch_pack_revoked_header_only() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/revoked-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(410)
                .insert_header("x-revocation-reason", "security vulnerability"),
        )
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("revoked-pack", "1.0.0", None).await;

    match result {
        Err(RegistryError::Revoked {
            name,
            version,
            reason,
            safe_version,
        }) => {
            assert_eq!(name, "revoked-pack");
            assert_eq!(version, "1.0.0");
            assert_eq!(reason, "security vulnerability");
            assert!(safe_version.is_none());
        }
        _ => panic!("expected Revoked error"),
    }
}

#[tokio::test]
async fn test_fetch_pack_revoked_with_body() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/revoked-pack/1.0.0"))
        .respond_with(ResponseTemplate::new(410).set_body_json(serde_json::json!({
            "reason": "critical CVE",
            "safe_version": "1.0.1"
        })))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("revoked-pack", "1.0.0", None).await;

    match result {
        Err(RegistryError::Revoked {
            name,
            version,
            reason,
            safe_version,
        }) => {
            assert_eq!(name, "revoked-pack");
            assert_eq!(version, "1.0.0");
            assert_eq!(reason, "critical CVE");
            assert_eq!(safe_version, Some("1.0.1".to_string()));
        }
        _ => panic!("expected Revoked error with safe_version"),
    }
}

#[tokio::test]
async fn test_rate_limiting_with_retry_after() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/rate-limited/1.0.0"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "5"))
        .mount(&mock_server)
        .await;

    let config = RegistryConfig {
        url: mock_server.uri(),
        token: Some("test-token".to_string()),
        max_retries: 0,
        ..Default::default()
    };
    let client = RegistryClient::new(config).expect("failed to create client");
    let result = client.fetch_pack("rate-limited", "1.0.0", None).await;

    match result {
        Err(RegistryError::RateLimited { retry_after }) => {
            assert_eq!(retry_after, Some(Duration::from_secs(5)));
        }
        _ => panic!("expected RateLimited error"),
    }
}

#[tokio::test]
async fn test_list_versions() {
    let mock_server = MockServer::start().await;

    let versions_json = serde_json::json!({
        "name": "test-pack",
        "versions": [
            {"version": "1.2.0", "digest": "sha256:abc123", "deprecated": false},
            {"version": "1.1.0", "digest": "sha256:def456", "deprecated": false},
            {"version": "1.0.0", "digest": "sha256:789abc", "deprecated": true}
        ]
    });

    Mock::given(method("GET"))
        .and(path("/packs/test-pack/versions"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&versions_json))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let response = client
        .list_versions("test-pack")
        .await
        .expect("list versions failed");

    assert_eq!(response.name, "test-pack");
    assert_eq!(response.versions.len(), 3);
    assert_eq!(response.versions[0].version, "1.2.0");
    assert!(response.versions[2].deprecated);
}

#[tokio::test]
async fn test_get_pack_meta() {
    let mock_server = MockServer::start().await;

    Mock::given(method("HEAD"))
        .and(path("/packs/test-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("x-pack-digest", "sha256:abc123")
                .insert_header("x-pack-signature", "dGVzdC1zaWduYXR1cmU=")
                .insert_header("x-pack-key-id", "sha256:keyid123")
                .insert_header("content-length", "1024"),
        )
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let meta = client
        .get_pack_meta("test-pack", "1.0.0")
        .await
        .expect("get meta failed");

    assert_eq!(meta.name, "test-pack");
    assert_eq!(meta.version, "1.0.0");
    assert_eq!(meta.digest, "sha256:abc123");
    assert!(meta.signed);
    assert_eq!(meta.key_id, Some("sha256:keyid123".to_string()));
    assert_eq!(meta.size, Some(1024));
}

#[tokio::test]
async fn test_fetch_keys_manifest() {
    let mock_server = MockServer::start().await;

    let keys_json = serde_json::json!({
        "version": 1,
        "keys": [
            {
                "key_id": "sha256:abc123",
                "algorithm": "Ed25519",
                "public_key": "dGVzdC1wdWJsaWMta2V5",
                "description": "Production signing key"
            }
        ]
    });

    Mock::given(method("GET"))
        .and(path("/keys"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&keys_json))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let manifest = client.fetch_keys().await.expect("fetch keys failed");

    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.keys.len(), 1);
    assert_eq!(manifest.keys[0].key_id, "sha256:abc123");
    assert_eq!(manifest.keys[0].algorithm, "Ed25519");
}

#[tokio::test]
async fn test_authentication_header() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/test/1.0.0"))
        .and(header("authorization", "Bearer secret-token"))
        .respond_with(ResponseTemplate::new(200).set_body_string("content"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = RegistryConfig::default()
        .with_url(mock_server.uri())
        .with_token("secret-token");
    let client = RegistryClient::new(config).expect("failed to create client");

    let result = client.fetch_pack("test", "1.0.0", None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_no_auth_when_no_token() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/public/1.0.0"))
        .respond_with(ResponseTemplate::new(200).set_body_string("content"))
        .mount(&mock_server)
        .await;

    let config = RegistryConfig {
        url: mock_server.uri(),
        token: None,
        ..Default::default()
    };
    let client = RegistryClient::new(config).expect("failed to create client");

    assert!(!client.is_authenticated());
    let result = client.fetch_pack("public", "1.0.0", None).await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn test_user_agent_header() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/test/1.0.0"))
        .and(header("user-agent", REGISTRY_USER_AGENT))
        .respond_with(ResponseTemplate::new(200).set_body_string("content"))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let _ = client.fetch_pack("test", "1.0.0", None).await;
}

#[tokio::test]
async fn test_fetch_signature_sidecar() {
    let mock_server = MockServer::start().await;

    let envelope = serde_json::json!({
        "payloadType": "application/vnd.assay.pack+yaml;v=1",
        "payload": "dGVzdCBwYXlsb2Fk",
        "signatures": [{
            "keyid": "sha256:abc123",
            "sig": "dGVzdCBzaWduYXR1cmU="
        }]
    });

    Mock::given(method("GET"))
        .and(path("/packs/signed-pack/1.0.0.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_signature("signed-pack", "1.0.0")
        .await
        .expect("fetch signature failed");

    let sig = result.expect("expected Some");
    assert_eq!(sig.payload_type, "application/vnd.assay.pack+yaml;v=1");
    assert_eq!(sig.signatures.len(), 1);
    assert_eq!(sig.signatures[0].key_id, "sha256:abc123");
}

#[tokio::test]
async fn test_fetch_signature_sidecar_not_found() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/unsigned-pack/1.0.0.sig"))
        .respond_with(ResponseTemplate::new(404))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_signature("unsigned-pack", "1.0.0")
        .await
        .expect("fetch signature should not error on 404");

    assert!(result.is_none(), "expected None for unsigned pack");
}

#[tokio::test]
async fn test_fetch_pack_with_signature_signature_500_error_bubbled() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: test-pack\nversion: \"1.0.0\"";
    let expected_digest = compute_digest(pack_yaml);

    Mock::given(method("GET"))
        .and(path("/packs/sig-500-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("x-pack-digest", expected_digest.as_str()),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/packs/sig-500-pack/1.0.0.sig"))
        .respond_with(ResponseTemplate::new(500).set_body_string("internal server error"))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_pack_with_signature("sig-500-pack", "1.0.0", None)
        .await;

    assert!(
        matches!(result, Err(RegistryError::Network { .. })),
        "signature 500 should bubble as Network error, not be swallowed"
    );
}

#[tokio::test]
async fn test_fetch_pack_with_signature_invalid_json_error_bubbled() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: test-pack\nversion: \"1.0.0\"";
    let expected_digest = compute_digest(pack_yaml);

    Mock::given(method("GET"))
        .and(path("/packs/sig-invalid-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("x-pack-digest", expected_digest.as_str()),
        )
        .mount(&mock_server)
        .await;

    Mock::given(method("GET"))
        .and(path("/packs/sig-invalid-pack/1.0.0.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_string("{not json"))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_pack_with_signature("sig-invalid-pack", "1.0.0", None)
        .await;

    assert!(
        matches!(result, Err(RegistryError::InvalidResponse { .. })),
        "invalid signature JSON should bubble as InvalidResponse, not be swallowed"
    );
}

#[tokio::test]
async fn test_fetch_pack_with_signature() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: signed-pack\nversion: \"1.0.0\"";
    let expected_digest = compute_digest(pack_yaml);

    Mock::given(method("GET"))
        .and(path("/packs/signed-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("x-pack-digest", expected_digest.as_str()),
        )
        .mount(&mock_server)
        .await;

    let envelope = serde_json::json!({
        "payloadType": "application/vnd.assay.pack+yaml;v=1",
        "payload": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pack_yaml),
        "signatures": [{
            "keyid": "sha256:key123",
            "sig": "dGVzdCBzaWduYXR1cmU="
        }]
    });

    Mock::given(method("GET"))
        .and(path("/packs/signed-pack/1.0.0.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_pack_with_signature("signed-pack", "1.0.0", None)
        .await
        .expect("fetch failed");

    let (fetch, sig) = result.expect("expected Some");
    assert_eq!(fetch.content, pack_yaml);
    assert!(sig.is_some());
    assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:key123");
}

#[tokio::test]
async fn test_commercial_pack_signature_required_via_sidecar_only() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: commercial-pack\nversion: \"1.0.0\"\nlicense: commercial";
    let expected_digest = compute_digest(pack_yaml);

    Mock::given(method("GET"))
        .and(path("/packs/commercial-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("x-pack-digest", expected_digest.as_str())
                .insert_header("x-pack-license", "LicenseRef-Assay-Enterprise-1.0")
                .insert_header(
                    "x-pack-signature-endpoint",
                    "/packs/commercial-pack/1.0.0.sig",
                ),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let envelope = serde_json::json!({
        "payloadType": "application/vnd.assay.pack+yaml;v=1",
        "payload": base64::Engine::encode(&base64::engine::general_purpose::STANDARD, pack_yaml),
        "signatures": [{
            "keyid": "sha256:commercial-key",
            "sig": "dGVzdCBzaWduYXR1cmU="
        }]
    });

    Mock::given(method("GET"))
        .and(path("/packs/commercial-pack/1.0.0.sig"))
        .respond_with(ResponseTemplate::new(200).set_body_json(&envelope))
        .expect(1)
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client
        .fetch_pack_with_signature("commercial-pack", "1.0.0", None)
        .await
        .expect("fetch failed");

    let (fetch, sig) = result.expect("expected Some");

    assert_eq!(fetch.content, pack_yaml);
    assert!(fetch.headers.signature.is_none());
    assert!(sig.is_some());
    assert_eq!(sig.unwrap().signatures[0].key_id, "sha256:commercial-key");
}

#[tokio::test]
async fn test_pack_304_signature_still_valid() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/cached-pack/1.0.0"))
        .and(header("if-none-match", "\"etag-abc\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;

    let result = client
        .fetch_pack("cached-pack", "1.0.0", Some("\"etag-abc\""))
        .await
        .expect("fetch failed");

    assert!(
        result.is_none(),
        "304 should return None - use cached pack+signature"
    );
}

#[tokio::test]
async fn test_etag_is_strong_etag_format() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: test\nversion: \"1.0.0\"";
    let digest = compute_digest(pack_yaml);
    let etag = format!("\"{}\"", digest);

    Mock::given(method("GET"))
        .and(path("/packs/test/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("etag", etag.as_str())
                .insert_header("x-pack-digest", digest.as_str()),
        )
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
    let fetch = result.unwrap();

    assert_eq!(fetch.headers.etag, Some(etag));
    let etag_unquoted = fetch.headers.etag.unwrap().trim_matches('"').to_string();
    assert_eq!(etag_unquoted, digest);
}

#[tokio::test]
async fn test_vary_header_for_authenticated_response() {
    let mock_server = MockServer::start().await;

    let pack_yaml = "name: test\nversion: \"1.0.0\"";

    Mock::given(method("GET"))
        .and(path("/packs/test/1.0.0"))
        .and(header("authorization", "Bearer test-token"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(pack_yaml)
                .insert_header("vary", "Authorization, Accept-Encoding")
                .insert_header("cache-control", "private, max-age=86400"),
        )
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("test", "1.0.0", None).await;

    assert!(result.is_ok());
}

#[tokio::test]
async fn test_content_digest_vs_canonical_digest() {
    let mock_server = MockServer::start().await;

    let wire_content = "name:   test\nversion:    \"1.0.0\"\n\n";
    let canonical_content = "name: test\nversion: \"1.0.0\"";
    let canonical_digest = compute_digest(canonical_content);

    Mock::given(method("GET"))
        .and(path("/packs/test/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(wire_content)
                .insert_header("x-pack-digest", canonical_digest.as_str()),
        )
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;
    let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
    let fetch = result.unwrap();

    assert_eq!(fetch.content, wire_content);
    assert_eq!(fetch.headers.digest, Some(canonical_digest.clone()));
    assert_eq!(fetch.computed_digest, canonical_digest);
}

#[tokio::test]
async fn test_304_cache_hit_flow() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/cached-pack/1.0.0"))
        .and(header("if-none-match", "\"sha256:abc123\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&mock_server)
        .await;

    let client = create_test_client(&mock_server).await;

    let result = client
        .fetch_pack("cached-pack", "1.0.0", Some("\"sha256:abc123\""))
        .await
        .unwrap();

    assert!(result.is_none(), "304 should return None - use cached pack");
}

#[tokio::test]
async fn test_retry_on_429_with_retry_after() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/retry-test/1.0.0"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .expect(2)
        .mount(&mock_server)
        .await;

    let config = RegistryConfig {
        url: mock_server.uri(),
        token: Some("test-token".to_string()),
        max_retries: 1,
        timeout_secs: 30,
        ..Default::default()
    };
    let client = RegistryClient::new(config).unwrap();

    let start = std::time::Instant::now();
    let result = client.fetch_pack("retry-test", "1.0.0", None).await;
    let elapsed = start.elapsed();

    assert!(
        matches!(result, Err(RegistryError::RateLimited { .. })),
        "Should fail with RateLimited"
    );

    assert!(
        elapsed.as_millis() >= 850,
        "Should have waited for retry-after (with jitter), elapsed: {:?}",
        elapsed
    );
}

#[tokio::test]
async fn test_max_retries_exceeded() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/fail-test/1.0.0"))
        .respond_with(ResponseTemplate::new(429).insert_header("retry-after", "1"))
        .expect(2)
        .mount(&mock_server)
        .await;

    let config = RegistryConfig {
        url: mock_server.uri(),
        token: Some("test-token".to_string()),
        max_retries: 1,
        timeout_secs: 30,
        ..Default::default()
    };
    let client = RegistryClient::new(config).unwrap();

    let result = client.fetch_pack("fail-test", "1.0.0", None).await;
    assert!(
        matches!(result, Err(RegistryError::RateLimited { .. })),
        "Should fail with RateLimited after max retries"
    );
}
