use assay_registry::{
    compute_digest, PackCache, PackResolver, RegistryClient, RegistryConfig, RegistryError,
    ResolveSource, ResolverConfig, TrustStore,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const ROOT_KEY_ID: &str = "sha256:3a64307d5655ba86fa3c95118ed8fe9665ef6bd37c752ca93f3bbe8f16e83a7f";
const ROOTED_TRUST_CONTENT: &str = "kind: compliance\nname: rooted-trust\nversion: \"1.0.0\"\n";
const ROOTED_TRUST_SIGNATURE_B64: &str = "eyJwYXlsb2FkVHlwZSI6ImFwcGxpY2F0aW9uL3ZuZC5hc3NheS5wYWNrK3lhbWw7dj0xIiwicGF5bG9hZCI6ImV5SnJhVzVrSWpvaVkyOXRjR3hwWVc1alpTSXNJbTVoYldVaU9pSnliMjkwWldRdGRISjFjM1FpTENKMlpYSnphVzl1SWpvaU1TNHdMakFpZlE9PSIsInNpZ25hdHVyZXMiOlt7ImtleWlkIjoic2hhMjU2OjNhNjQzMDdkNTY1NWJhODZmYTNjOTUxMThlZDhmZTk2NjVlZjZiZDM3Yzc1MmNhOTNmM2JiZThmMTZlODNhN2YiLCJzaWciOiJiWm5lVlhYKzBpdVhucDBXVGVYUDBOemxtdUl1Wkp5MUJ5OTFlMm9Zb05pQTNvNXRsQXB5Lytib253VUZCQUVzeHVnTWlXdVJsRWxBZEtRQ1YwbXdEUT09In1dfQ==";

fn test_resolver_config(mock_server: &MockServer) -> ResolverConfig {
    ResolverConfig {
        registry: RegistryConfig::default()
            .with_url(mock_server.uri())
            .with_token("test-token"),
        no_cache: true,
        allow_unsigned: false,
        bundled_packs_dir: None,
    }
}

fn resolver_with_production_roots(
    mock_server: &MockServer,
    cache_dir: &TempDir,
) -> assay_registry::RegistryResult<PackResolver> {
    let config = test_resolver_config(mock_server);
    let client = RegistryClient::new(config.registry.clone())?;
    let cache = PackCache::with_dir(cache_dir.path().join("cache"));
    let trust_store = TrustStore::from_production_roots()?;
    Ok(PackResolver::with_components(
        client,
        cache,
        trust_store,
        config,
    ))
}

fn envelope_with_key_id(key_id: &str) -> String {
    let bytes = BASE64.decode(ROOTED_TRUST_SIGNATURE_B64).unwrap();
    let mut envelope: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    envelope["signatures"][0]["keyid"] = serde_json::Value::String(key_id.to_string());
    BASE64.encode(serde_json::to_vec(&envelope).unwrap())
}

#[tokio::test]
async fn resolver_accepts_signed_pack_with_embedded_production_root() {
    let mock_server = MockServer::start().await;
    let cache_dir = TempDir::new().unwrap();
    let digest = compute_digest(ROOTED_TRUST_CONTENT);

    Mock::given(method("GET"))
        .and(path("/packs/rooted-trust/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ROOTED_TRUST_CONTENT)
                .insert_header("x-pack-digest", digest.as_str())
                .insert_header("x-pack-signature", ROOTED_TRUST_SIGNATURE_B64)
                .insert_header("x-pack-key-id", ROOT_KEY_ID),
        )
        .mount(&mock_server)
        .await;

    let resolver = resolver_with_production_roots(&mock_server, &cache_dir).unwrap();
    let resolved = resolver.resolve("rooted-trust@1.0.0").await.unwrap();

    assert!(matches!(resolved.source, ResolveSource::Registry(_)));
    let verification = resolved.verification.expect("signed pack should verify");
    assert_eq!(verification.key_id.as_deref(), Some(ROOT_KEY_ID));
    assert_eq!(verification.digest, digest);
}

#[tokio::test]
async fn resolver_rejects_signed_pack_with_untrusted_key_id() {
    let mock_server = MockServer::start().await;
    let cache_dir = TempDir::new().unwrap();
    let untrusted_signature = envelope_with_key_id("sha256:deadbeef");

    Mock::given(method("GET"))
        .and(path("/packs/rooted-trust/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(ROOTED_TRUST_CONTENT)
                .insert_header(
                    "x-pack-digest",
                    compute_digest(ROOTED_TRUST_CONTENT).as_str(),
                )
                .insert_header("x-pack-signature", untrusted_signature)
                .insert_header("x-pack-key-id", "sha256:deadbeef"),
        )
        .mount(&mock_server)
        .await;

    let resolver = resolver_with_production_roots(&mock_server, &cache_dir).unwrap();
    let err = resolver.resolve("rooted-trust@1.0.0").await.unwrap_err();

    assert!(matches!(err, RegistryError::KeyNotTrusted { .. }));
}
