use super::*;

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

    let client = support::create_test_client(&mock_server).await;
    let _ = client.fetch_pack("test", "1.0.0", None).await;
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

    let client = support::create_test_client(&mock_server).await;
    let result = client.fetch_pack("test", "1.0.0", None).await;

    assert!(result.is_ok());
}
