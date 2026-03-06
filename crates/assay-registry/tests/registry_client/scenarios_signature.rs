use super::*;

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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;
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
