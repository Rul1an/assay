use super::*;

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
