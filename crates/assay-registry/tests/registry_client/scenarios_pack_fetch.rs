use super::*;

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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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
