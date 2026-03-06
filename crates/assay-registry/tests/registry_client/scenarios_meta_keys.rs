use super::*;

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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
    let manifest = client.fetch_keys().await.expect("fetch keys failed");

    assert_eq!(manifest.version, 1);
    assert_eq!(manifest.keys.len(), 1);
    assert_eq!(manifest.keys[0].key_id, "sha256:abc123");
    assert_eq!(manifest.keys[0].algorithm, "Ed25519");
}
