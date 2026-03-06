use super::*;

#[tokio::test]
async fn test_pack_304_signature_still_valid() {
    let mock_server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/packs/cached-pack/1.0.0"))
        .and(header("if-none-match", "\"etag-abc\""))
        .respond_with(ResponseTemplate::new(304))
        .mount(&mock_server)
        .await;

    let client = support::create_test_client(&mock_server).await;

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

    let client = support::create_test_client(&mock_server).await;
    let result = client.fetch_pack("test", "1.0.0", None).await.unwrap();
    let fetch = result.unwrap();

    assert_eq!(fetch.headers.etag, Some(etag));
    let etag_unquoted = fetch.headers.etag.unwrap().trim_matches('"').to_string();
    assert_eq!(etag_unquoted, digest);
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

    let client = support::create_test_client(&mock_server).await;
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

    let client = support::create_test_client(&mock_server).await;

    let result = client
        .fetch_pack("cached-pack", "1.0.0", Some("\"sha256:abc123\""))
        .await
        .unwrap();

    assert!(result.is_none(), "304 should return None - use cached pack");
}
