use super::*;

pub(super) async fn create_test_client(mock_server: &MockServer) -> RegistryClient {
    let config = RegistryConfig::default()
        .with_url(mock_server.uri())
        .with_token("test-token");
    RegistryClient::new(config).expect("failed to create client")
}
