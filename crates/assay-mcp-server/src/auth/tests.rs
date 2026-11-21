#[test]
fn test_ssrf_ip_blocking() {
    use crate::auth::jwks::JwksProvider;
    use url::Url;

    let unsafe_uris = vec![
        "https://127.0.0.1/jwks.json",
        "https://[::1]/jwks.json",
        "https://10.0.0.5/jwks",
        "https://192.168.1.1/jwks",
        "https://172.16.0.1/jwks",                  // Lower bound
        "https://172.31.255.255/jwks",              // Upper bound
        "https://169.254.169.254/latest/meta-data", // Cloud metadata
    ];

    for u in unsafe_uris {
        let url = Url::parse(u).unwrap();
        match JwksProvider::new(url) {
            Err(e) => assert!(e.to_string().contains("unsafe IP")),
            Ok(_) => panic!("Should block unsafe URI: {}", u),
        }
    }

    // Safe URIs
    let safe_uris = vec![
        "https://auth.example.com/jwks",
        "https://8.8.8.8/jwks", // Public IP
    ];
    for u in safe_uris {
        let url = Url::parse(u).unwrap();
        let res = JwksProvider::new(url);
        assert!(res.is_ok(), "Should allow safe URI: {}", u);
    }
}

#[test]
fn claims_deserializes_subject_and_resource() {
    let claims: super::Claims = serde_json::from_value(serde_json::json!({
        "sub": "client",
        "exp": 1,
        "resource": "urn:example:service",
    }))
    .unwrap();
    assert_eq!(claims.sub, "client");
    assert_eq!(
        claims.resource.as_ref().and_then(serde_json::Value::as_str),
        Some("urn:example:service")
    );
}
