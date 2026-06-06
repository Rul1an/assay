use assay_registry::{
    compute_digest, FetchResult, PackCache, PackHeaders, PackResolver, RegistryClient,
    RegistryConfig, ResolveSource, ResolverConfig, TrustStore,
};
use tempfile::TempDir;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

fn resolver_config(mock_server: &MockServer) -> ResolverConfig {
    ResolverConfig {
        registry: RegistryConfig::default()
            .with_url(mock_server.uri())
            .with_token("test-token"),
        no_cache: false,
        allow_unsigned: true,
        bundled_packs_dir: None,
    }
}

fn fetch_result(content: &str, etag: Option<&str>) -> FetchResult {
    let digest = compute_digest(content);
    FetchResult {
        content: content.to_string(),
        headers: PackHeaders {
            digest: Some(digest.clone()),
            signature: None,
            key_id: None,
            etag: etag.map(str::to_string),
            cache_control: Some("max-age=3600".to_string()),
            content_length: Some(content.len() as u64),
        },
        computed_digest: digest,
    }
}

fn resolver_with_cache(
    mock_server: &MockServer,
    cache: PackCache,
) -> assay_registry::RegistryResult<PackResolver> {
    let config = resolver_config(mock_server);
    let client = RegistryClient::new(config.registry.clone())?;
    Ok(PackResolver::with_components(
        client,
        cache,
        TrustStore::new(),
        config,
    ))
}

#[tokio::test]
async fn resolver_uses_fresh_cache_before_network() -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new()?;
    let cache = PackCache::with_dir(temp_dir.path().join("cache"));
    let cached_content = "name: cached-pack\nversion: \"1.0.0\"\n";
    cache
        .put(
            "cached-pack",
            "1.0.0",
            &fetch_result(cached_content, Some("\"cache-etag\"")),
            Some("https://cache.example.test"),
        )
        .await?;

    let resolver = resolver_with_cache(&mock_server, cache)?;
    let resolved = resolver.resolve("cached-pack@1.0.0").await?;

    assert_eq!(resolved.source, ResolveSource::Cache);
    assert_eq!(resolved.content, cached_content);
    assert_eq!(resolved.digest, compute_digest(cached_content));
    Ok(())
}

#[tokio::test]
async fn resolver_evicts_pinned_cache_mismatch_and_refetches(
) -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new()?;
    let cache = PackCache::with_dir(temp_dir.path().join("cache"));
    let stale_content = "name: pinned-pack\nversion: \"1.0.0\"\nstate: stale\n";
    let fresh_content = "name: pinned-pack\nversion: \"1.0.0\"\nstate: fresh\n";
    let fresh_digest = compute_digest(fresh_content);

    cache
        .put(
            "pinned-pack",
            "1.0.0",
            &fetch_result(stale_content, Some("\"stale-etag\"")),
            Some("https://cache.example.test"),
        )
        .await?;

    Mock::given(method("GET"))
        .and(path("/packs/pinned-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fresh_content)
                .insert_header("x-pack-digest", fresh_digest.as_str()),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let resolver = resolver_with_cache(&mock_server, cache)?;
    let resolved = resolver
        .resolve(&format!("pinned-pack@1.0.0#{}", fresh_digest))
        .await?;

    assert!(matches!(resolved.source, ResolveSource::Registry(_)));
    assert_eq!(resolved.content, fresh_content);
    assert_eq!(resolved.digest, fresh_digest);
    Ok(())
}

#[tokio::test]
async fn resolver_no_cache_skips_cached_entry_and_fetches_registry(
) -> Result<(), Box<dyn std::error::Error>> {
    let mock_server = MockServer::start().await;
    let temp_dir = TempDir::new()?;
    let cache = PackCache::with_dir(temp_dir.path().join("cache"));
    let cached_content = "name: no-cache-pack\nversion: \"1.0.0\"\nstate: cached\n";
    let fresh_content = "name: no-cache-pack\nversion: \"1.0.0\"\nstate: fresh\n";
    let fresh_digest = compute_digest(fresh_content);
    cache
        .put(
            "no-cache-pack",
            "1.0.0",
            &fetch_result(cached_content, Some("\"cache-etag\"")),
            Some("https://cache.example.test"),
        )
        .await?;

    Mock::given(method("GET"))
        .and(path("/packs/no-cache-pack/1.0.0"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_body_string(fresh_content)
                .insert_header("x-pack-digest", fresh_digest.as_str()),
        )
        .expect(1)
        .mount(&mock_server)
        .await;

    let config = ResolverConfig {
        no_cache: true,
        ..resolver_config(&mock_server)
    };
    let client = RegistryClient::new(config.registry.clone())?;
    let resolver = PackResolver::with_components(client, cache, TrustStore::new(), config);
    let resolved = resolver.resolve("no-cache-pack@1.0.0").await?;

    assert!(matches!(resolved.source, ResolveSource::Registry(_)));
    assert_eq!(resolved.content, fresh_content);
    assert_eq!(resolved.digest, fresh_digest);
    Ok(())
}
