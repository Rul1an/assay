//! Policy parity across registry fetch, prefetch and persistent cache reuse.
use assay_registry::types::{KeysManifest, TrustedKey};
use assay_registry::{
    compute_digest, FetchResult, PackCache, PackHeaders, PackResolver, RegistryClient,
    RegistryConfig, ResolveSource, ResolverConfig, TrustStore,
};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine};
use ed25519_dalek::{pkcs8::EncodePublicKey, Signer, SigningKey};
use tempfile::TempDir;
use wiremock::{
    matchers::{method, path},
    Mock, MockServer, ResponseTemplate,
};

type Result<T = ()> = std::result::Result<T, Box<dyn std::error::Error>>;
const CONTENT: &str = "name: policy-pack\nversion: '1.0.0'\n";
const REF: &str = "policy-pack@1.0.0";

fn fetched(signature: Option<String>) -> FetchResult {
    FetchResult {
        content: CONTENT.into(),
        computed_digest: compute_digest(CONTENT),
        headers: PackHeaders {
            digest: Some(compute_digest(CONTENT)),
            signature,
            key_id: None,
            etag: Some("\"cached\"".into()),
            cache_control: Some("max-age=3600".into()),
            content_length: None,
        },
    }
}
fn resolver(
    server: &MockServer,
    cache: &PackCache,
    trust: TrustStore,
    unsigned: bool,
    no_cache: bool,
) -> Result<PackResolver> {
    let config = ResolverConfig {
        registry: RegistryConfig::default()
            .with_url(server.uri())
            .with_token("test"),
        no_cache,
        allow_unsigned: unsigned,
        bundled_packs_dir: None,
    };
    Ok(PackResolver::with_components(
        RegistryClient::new(config.registry.clone())?,
        cache.clone(),
        trust,
        config,
    ))
}
async fn respond(server: &MockServer, result: &FetchResult) {
    let mut response = ResponseTemplate::new(200).set_body_string(&result.content);
    if let Some(digest) = &result.headers.digest {
        response = response.insert_header("x-pack-digest", digest.as_str());
    }
    if let Some(sig) = &result.headers.signature {
        response = response.insert_header("x-pack-signature", sig.as_str());
    }
    Mock::given(method("GET"))
        .and(path("/packs/policy-pack/1.0.0"))
        .respond_with(response)
        .mount(server)
        .await;
}
fn signed() -> Result<(FetchResult, TrustedKey)> {
    let key = SigningKey::from_bytes(&[7; 32]);
    let der = key.verifying_key().to_public_key_der()?;
    let id = assay_registry::verify::compute_key_id(der.as_bytes());
    let payload = br#"{"name":"policy-pack","version":"1.0.0"}"#;
    let ty = assay_registry::verify::PAYLOAD_TYPE_PACK_V1;
    let pae = assay_common::dsse::build_pae(ty, payload);
    let envelope = serde_json::json!({"payloadType":ty,"payload":BASE64.encode(payload),
        "signatures":[{"keyid":id,"sig":BASE64.encode(key.sign(&pae).to_bytes())}]});
    Ok((
        fetched(Some(BASE64.encode(serde_json::to_vec(&envelope)?))),
        TrustedKey {
            key_id: id,
            algorithm: "Ed25519".into(),
            public_key: BASE64.encode(der.as_bytes()),
            description: None,
            added_at: None,
            expires_at: None,
            revoked: false,
        },
    ))
}
async fn trust(key: TrustedKey) -> Result<TrustStore> {
    let store = TrustStore::new();
    store
        .add_from_manifest(&KeysManifest {
            version: 1,
            keys: vec![key],
            expires_at: None,
        })
        .await?;
    Ok(store)
}

#[tokio::test]
async fn strict_prefetch_rejects_unsigned_without_populating_cache() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    respond(&s, &fetched(None)).await;
    let r = resolver(&s, &c, TrustStore::new(), false, false)?;
    assert!(r.resolve(REF).await.is_err(), "fresh strict control");
    assert!(
        r.prefetch(REF).await.is_err(),
        "prefetch must enforce strict policy"
    );
    assert!(c.get("policy-pack", "1.0.0").await?.is_none());
    Ok(())
}
#[tokio::test]
async fn permissive_prefetch_cannot_bypass_later_strict_policy() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    respond(&s, &fetched(None)).await;
    resolver(&s, &c, TrustStore::new(), true, false)?
        .prefetch(REF)
        .await?;
    assert!(resolver(&s, &c, TrustStore::new(), false, false)?
        .resolve(REF)
        .await
        .is_err());
    Ok(())
}
#[tokio::test]
async fn cached_signature_uses_current_trust_and_preserves_offline_success() -> Result {
    for state in ["valid", "removed", "revoked", "expired"] {
        let s = MockServer::start().await;
        let t = TempDir::new()?;
        let c = PackCache::with_dir(t.path());
        let (f, mut key) = signed()?;
        respond(&s, &f).await;
        let store = trust(key.clone()).await?;
        let r = resolver(&s, &c, store.clone(), false, false)?;
        assert!(r.resolve(REF).await?.verification.unwrap().signed);
        s.reset().await;
        match state {
            "removed" => store.clear_cached_keys().await,
            "revoked" | "expired" => {
                key.revoked = state == "revoked";
                if state == "expired" {
                    key.expires_at = Some(chrono::Utc::now() + chrono::Duration::seconds(2));
                }
                store
                    .add_from_manifest(&KeysManifest {
                        version: 1,
                        keys: vec![key],
                        expires_at: None,
                    })
                    .await?;
                if state == "expired" {
                    // Let an admitted key expire; an already-expired manifest
                    // entry is ignored by TrustStore rather than replacing it.
                    tokio::time::sleep(std::time::Duration::from_millis(2100)).await;
                }
            }
            _ => {}
        }
        let result = r.resolve(REF).await;
        if state == "valid" {
            let pack = result?;
            assert_eq!(pack.source, ResolveSource::Cache);
            assert!(pack.verification.unwrap().signed);
        } else {
            assert!(result.is_err(), "cached {state} key accepted");
        }
    }
    Ok(())
}
#[tokio::test]
async fn cache_origin_must_match_and_never_leak_etag() -> Result {
    for origin in [None, Some("https://other.example.test")] {
        let s = MockServer::start().await;
        let t = TempDir::new()?;
        let c = PackCache::with_dir(t.path());
        c.put("policy-pack", "1.0.0", &fetched(None), origin)
            .await?;
        respond(&s, &fetched(None)).await;
        let p = resolver(&s, &c, TrustStore::new(), true, false)?
            .resolve(REF)
            .await?;
        assert!(matches!(p.source, ResolveSource::Registry(_)));
        let requests = s.received_requests().await.unwrap();
        assert_eq!(requests.len(), 1);
        assert!(!requests[0].headers.contains_key("if-none-match"));
    }
    Ok(())
}
#[tokio::test]
async fn pinned_digest_is_enforced_by_prefetch_and_resolve() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    respond(&s, &fetched(None)).await;
    let r = resolver(&s, &c, TrustStore::new(), true, false)?;
    let bad = format!("{REF}#sha256:{}", "0".repeat(64));
    assert!(r.resolve(&bad).await.is_err());
    assert!(r.prefetch(&bad).await.is_err());
    r.prefetch(REF).await?;
    assert!(r.resolve(&bad).await.is_err());
    let good = format!("{REF}#{}", compute_digest(CONTENT));
    r.prefetch(&good).await?;
    assert_eq!(r.resolve(&good).await?.digest, compute_digest(CONTENT));
    Ok(())
}
#[tokio::test]
async fn unsigned_option_never_allows_invalid_signature_or_claimed_digest() -> Result {
    for bad_signature in [true, false] {
        let s = MockServer::start().await;
        let t = TempDir::new()?;
        let c = PackCache::with_dir(t.path());
        let mut f = fetched(None);
        if bad_signature {
            f.headers.signature = Some("not-a-signature".into());
        } else {
            f.headers.digest = Some(format!("sha256:{}", "0".repeat(64)));
        }
        respond(&s, &f).await;
        let r = resolver(&s, &c, TrustStore::new(), true, false)?;
        assert!(r.resolve(REF).await.is_err());
        assert!(r.prefetch(REF).await.is_err());
        assert!(c.get("policy-pack", "1.0.0").await?.is_none());
    }
    Ok(())
}
#[tokio::test]
async fn unsolicited_304_cannot_resurrect_cache_even_with_no_cache() -> Result {
    for no_cache in [true, false] {
        let s = MockServer::start().await;
        let t = TempDir::new()?;
        let c = PackCache::with_dir(t.path());
        c.put("policy-pack", "1.0.0", &fetched(None), Some(&s.uri()))
            .await?;
        Mock::given(method("GET"))
            .respond_with(ResponseTemplate::new(304))
            .mount(&s)
            .await;
        let r = resolver(&s, &c, TrustStore::new(), false, no_cache)?;
        assert!(r.resolve(REF).await.is_err());
        assert!(r.prefetch(REF).await.is_err());
    }
    Ok(())
}
#[tokio::test]
async fn sidecar_corruption_is_not_unsigned_and_unsigned_replacement_removes_it() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    let (f, _) = signed()?;
    c.put("policy-pack", "1.0.0", &f, Some(&s.uri())).await?;
    c.put("policy-pack", "1.0.0", &fetched(None), Some(&s.uri()))
        .await?;
    assert!(c
        .get("policy-pack", "1.0.0")
        .await?
        .unwrap()
        .signature
        .is_none());
    let sig = t.path().join("policy-pack/1.0.0/signature.json");
    tokio::fs::write(&sig, "{").await?;
    assert!(c.get("policy-pack", "1.0.0").await.is_err());
    assert!(resolver(&s, &c, TrustStore::new(), true, false)?
        .resolve(REF)
        .await
        .is_err());
    tokio::fs::remove_file(&sig).await?;
    tokio::fs::create_dir(&sig).await?;
    assert!(c.get("policy-pack", "1.0.0").await.is_err());
    Ok(())
}

#[tokio::test]
async fn prefetch_warms_cache_even_when_normal_resolution_disables_cache() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    let (f, key) = signed()?;
    respond(&s, &f).await;
    let store = trust(key).await?;
    resolver(&s, &c, store.clone(), false, true)?
        .prefetch(REF)
        .await?;
    s.reset().await;
    let pack = resolver(&s, &c, store, false, false)?.resolve(REF).await?;
    assert_eq!(pack.source, ResolveSource::Cache);
    assert!(pack.verification.unwrap().signed);
    Ok(())
}

#[tokio::test]
async fn invalid_present_signatures_fail_on_network_and_cache() -> Result {
    for mode in ["unknown-key", "bad-signature", "wrong-payload"] {
        let s = MockServer::start().await;
        let t = TempDir::new()?;
        let c = PackCache::with_dir(t.path());
        let (mut f, key) = signed()?;
        let mut env: serde_json::Value =
            serde_json::from_slice(&BASE64.decode(f.headers.signature.as_ref().unwrap())?)?;
        match mode {
            "unknown-key" => env["signatures"][0]["keyid"] = "sha256:unknown".into(),
            "bad-signature" => env["signatures"][0]["sig"] = BASE64.encode([0_u8; 64]).into(),
            "wrong-payload" => env["payload"] = BASE64.encode(b"{}").into(),
            _ => unreachable!(),
        }
        f.headers.signature = Some(BASE64.encode(serde_json::to_vec(&env)?));
        respond(&s, &f).await;
        let r = resolver(&s, &c, trust(key).await?, true, false)?;
        assert!(r.resolve(REF).await.is_err(), "fresh {mode}");
        assert!(r.prefetch(REF).await.is_err(), "prefetch {mode}");
        c.put("policy-pack", "1.0.0", &f, Some(&s.uri())).await?;
        assert!(r.resolve(REF).await.is_err(), "cached {mode}");
    }
    Ok(())
}

#[tokio::test]
async fn expired_cache_refetches_without_conditional_headers() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    let mut f = fetched(None);
    f.headers.cache_control = Some("max-age=-1".into());
    c.put("policy-pack", "1.0.0", &f, Some(&s.uri())).await?;
    respond(&s, &fetched(None)).await;
    let r = resolver(&s, &c, TrustStore::new(), true, false)?;
    let pack = r.resolve(REF).await?;
    assert!(matches!(pack.source, ResolveSource::Registry(_)));
    assert!(!pack.verification.unwrap().signed);
    let requests = s.received_requests().await.unwrap();
    assert_eq!(requests.len(), 1);
    assert!(!requests[0].headers.contains_key("if-none-match"));
    s.reset().await;
    let pack = r.resolve(REF).await?;
    assert_eq!(pack.source, ResolveSource::Cache);
    assert!(!pack.verification.unwrap().signed);
    Ok(())
}

#[tokio::test]
async fn malformed_signature_cannot_replace_a_valid_cache_entry() -> Result {
    let t = TempDir::new()?;
    let c = PackCache::with_dir(t.path());
    let (f, _) = signed()?;
    c.put("policy-pack", "1.0.0", &f, None).await?;
    let mut bad = fetched(Some("invalid".into()));
    bad.content = "name: replacement\n".into();
    bad.computed_digest = compute_digest(&bad.content);
    assert!(c.put("policy-pack", "1.0.0", &bad, None).await.is_err());
    let entry = c.get("policy-pack", "1.0.0").await?.unwrap();
    assert_eq!(entry.content, CONTENT);
    assert!(entry.signature.is_some());
    Ok(())
}

#[tokio::test]
async fn prefetch_reports_cache_write_failure() -> Result {
    let s = MockServer::start().await;
    let t = TempDir::new()?;
    let blocked = t.path().join("not-a-directory");
    tokio::fs::write(&blocked, b"occupied").await?;
    let c = PackCache::with_dir(blocked);
    respond(&s, &fetched(None)).await;
    assert!(resolver(&s, &c, TrustStore::new(), true, false)?
        .prefetch(REF)
        .await
        .is_err());
    Ok(())
}
