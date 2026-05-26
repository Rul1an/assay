use super::*;
use tempfile::TempDir;

/// Sign-off: default_secure() must scrub auth and common secret headers so cassettes don't leak.
#[test]
fn test_default_secure_scrub_paths() {
    let cfg = ScrubConfig::default_secure();
    assert!(
        cfg.request_headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case("authorization")),
        "Must scrub Authorization"
    );
    assert!(
        cfg.request_headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case("x-api-key")),
        "Must scrub x-api-key"
    );
    assert!(
        cfg.request_headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case("api-key")),
        "Must scrub api-key"
    );
    assert!(
        cfg.response_headers
            .iter()
            .any(|h| h.eq_ignore_ascii_case("set-cookie")),
        "Must scrub set-cookie"
    );
    assert!(
        cfg.request_body_paths.is_empty(),
        "Default: no body paths (audit: explicit if needed)"
    );
    assert!(
        cfg.response_body_paths.is_empty(),
        "Default: no response body paths"
    );
}

#[test]
fn test_fingerprint_stability() {
    let body = serde_json::json!({"input": "hello", "model": "text-embedding-3-small"});
    let fp1 = VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));
    let fp2 = VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));
    assert_eq!(fp1, fp2);

    // Different body = different fingerprint
    let body2 = serde_json::json!({"input": "world", "model": "text-embedding-3-small"});
    let fp3 = VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body2));
    assert_ne!(fp1, fp3);
}

#[test]
fn test_fingerprint_key_order_invariant() {
    // JCS ensures key order doesn't matter
    let body1 = serde_json::json!({"model": "gpt-4", "input": "hello"});
    let body2 = serde_json::json!({"input": "hello", "model": "gpt-4"});
    let fp1 = VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body1));
    let fp2 = VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body2));
    assert_eq!(fp1, fp2, "JCS should normalize key order");
}

#[test]
fn test_vcr_mode_from_env() {
    env::remove_var("ASSAY_VCR_MODE");
    assert_eq!(VcrMode::from_env(), VcrMode::ReplayStrict);

    env::set_var("ASSAY_VCR_MODE", "record");
    assert_eq!(VcrMode::from_env(), VcrMode::Record);

    env::set_var("ASSAY_VCR_MODE", "auto");
    assert_eq!(VcrMode::from_env(), VcrMode::Auto);

    env::set_var("ASSAY_VCR_MODE", "replay");
    assert_eq!(VcrMode::from_env(), VcrMode::Replay);

    env::set_var("ASSAY_VCR_MODE", "off");
    assert_eq!(VcrMode::from_env(), VcrMode::Off);

    env::set_var("ASSAY_VCR_MODE", "replay_strict");
    assert_eq!(VcrMode::from_env(), VcrMode::ReplayStrict);

    env::remove_var("ASSAY_VCR_MODE");
}

#[test]
fn test_cassette_save_load_atomic() {
    let tmp = TempDir::new().unwrap();
    let client = VcrClient::new(VcrMode::Record, tmp.path().to_path_buf());

    let body = serde_json::json!({"input": "test", "model": "text-embedding-3-small"});
    let fingerprint =
        VcrClient::fingerprint("POST", "https://api.openai.com/v1/embeddings", Some(&body));

    let entry = CassetteEntry {
        schema_version: 2,
        fingerprint: fingerprint.clone(),
        method: "POST".to_string(),
        url: "https://api.openai.com/v1/embeddings".to_string(),
        request_body: Some(body),
        status: 200,
        response_body: serde_json::json!({"data": [{"embedding": [0.1, 0.2]}]}),
        meta: CassetteMeta {
            recorded_at: "2026-01-30T12:00:00Z".to_string(),
            model: Some("text-embedding-3-small".to_string()),
            provider: "openai".to_string(),
            kind: "embeddings".to_string(),
        },
    };

    client.save_cassette(&entry).unwrap();

    // Verify file exists in correct location
    let expected_path = tmp
        .path()
        .join("openai")
        .join("embeddings")
        .join(format!("{}.json", &fingerprint[..16]));
    assert!(expected_path.exists(), "Cassette file should exist");

    // Reload and verify
    let mut client2 = VcrClient::new(VcrMode::ReplayStrict, tmp.path().to_path_buf());
    client2.load_cassettes();

    assert!(client2.cache.contains_key(&fingerprint));
    assert_eq!(client2.cache.get(&fingerprint).unwrap().status, 200);
}

#[test]
fn test_provider_and_kind_detection() {
    assert_eq!(
        VcrClient::provider_from_url("https://api.openai.com/v1/embeddings"),
        "openai"
    );
    assert_eq!(
        VcrClient::kind_from_url("https://api.openai.com/v1/embeddings"),
        "embeddings"
    );
    assert_eq!(
        VcrClient::kind_from_url("https://api.openai.com/v1/chat/completions"),
        "judge"
    );
}

#[tokio::test]
async fn test_network_policy_blocks_passthrough_modes() {
    let _serial = crate::providers::network::lock_test_serial_async().await;
    let tmp = TempDir::new().unwrap();
    let mut client = VcrClient::new(VcrMode::Off, tmp.path().to_path_buf());
    let _guard = crate::providers::network::NetworkPolicyGuard::deny("unit test");
    let body = serde_json::json!({"input": "test", "model": "gpt-4o-mini"});
    let err = client
        .post_json("https://api.openai.com/v1/chat/completions", &body, None)
        .await
        .expect_err("deny policy must block passthrough network");
    assert!(err
        .to_string()
        .contains("outbound network blocked by policy"));
}
