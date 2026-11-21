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
            Some(&mock_server.uri()),
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
            Some(&mock_server.uri()),
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
            Some(&mock_server.uri()),
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

#[tokio::test]
async fn lockfile_update_only_discards_fetch_metadata_when_digest_changes(
) -> Result<(), Box<dyn std::error::Error>> {
    let server = MockServer::start().await;
    let dir = TempDir::new()?;
    let cache = PackCache::with_dir(dir.path().join("cache"));
    let fetched = fetch_result("name: cached-pack\nversion: '1.0.0'\n", None);
    cache
        .put("cached-pack", "1.0.0", &fetched, Some(&server.uri()))
        .await?;
    let resolver = resolver_with_cache(&server, cache)?;
    let mut lock =
        assay_registry::generate_lockfile(&["cached-pack@1.0.0".into()], &resolver).await?;
    let pack = &mut lock.packs[0];
    assert!(pack.namespace.is_none() && pack.fetched_at.is_none() && pack.etag.is_none());
    pack.fetched_at = Some("2026-01-29T10:00:00Z".into());
    pack.etag = Some("\"old-tag\"".into());
    pack.signature = Some(assay_registry::LockSignature {
        algorithm: "Ed25519".into(),
        key_id: "previous-signer".into(),
    });
    pack.registry_url = Some("https://previous-registry.example".into());
    let original = pack.clone();

    assert!(
        assay_registry::lockfile::update_lockfile(&mut lock, &resolver)
            .await?
            .is_empty()
    );
    assert_eq!(lock.packs[0], original);

    lock.packs[0].digest = "old-digest".into();
    assert_eq!(
        assay_registry::lockfile::update_lockfile(&mut lock, &resolver).await?,
        ["cached-pack"]
    );
    assert_eq!(lock.packs[0].digest, fetched.computed_digest);
    assert!(lock.packs[0].fetched_at.is_none() && lock.packs[0].etag.is_none());
    assert!(lock.packs[0].signature.is_none());
    assert!(lock.packs[0].registry_url.is_none());

    // The whole operation must refuse before it changes an earlier unscoped entry.
    lock.packs[0].digest = "restore-me".into();
    let mut scoped = original;
    scoped.namespace = Some(Some("orgs/acme".into()));
    lock.packs.push(scoped);
    let before = lock.to_yaml()?;
    let error = assay_registry::lockfile::update_lockfile(&mut lock, &resolver)
        .await
        .unwrap_err();
    assert!(error.to_string().contains("namespace"), "{error}");
    assert_eq!(lock.to_yaml()?, before);
    assert!(server.received_requests().await.unwrap().is_empty());
    Ok(())
}
