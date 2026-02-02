use super::config::{AuthConfig, AuthMode};
use super::validation::TokenValidator;
use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
use serde_json::json;
use std::time::{SystemTime, UNIX_EPOCH};

fn create_token(header: Header, claims: serde_json::Value) -> String {
    encode(
        &header,
        &claims,
        &EncodingKey::from_secret(b"test_secret_for_unit_testing_only"),
    )
    .unwrap()
}

fn valid_claims() -> serde_json::Value {
    json!({
        "sub": "user123",
        "iss": "https://auth.example.com",
        "aud": "assay-mcp",
        "exp": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() + 3600,
        "iat": SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs(),
        "resource": "assay-system"
    })
}

#[tokio::test]
async fn test_alg_whitelist_enforcement() {
    let mut header = Header::new(Algorithm::HS256);
    header.alg = Algorithm::HS256; // We use HS256 to sign, but Validator expects RS/ES.
                                   // Actually, `jsonwebtoken` crate doesn't easily let us forge "none" signed tokens that verify() checks
                                   // without `insecure_disable_signature_validation`.
                                   // But our Validator explicitly checks `header.alg` whitelist BEFORE verification.

    // Let's test the whitelist logic.
    let claims = valid_claims();
    let token = create_token(header, claims);

    let validator = TokenValidator::new(None); // No JWKS needed for this check failure
    let config = AuthConfig {
        mode: AuthMode::Strict,
        ..Default::default()
    };

    // HS256 (symmetric) is NOT in our whitelist (RS256/ES256 only for SOTA)
    let res = validator.validate(&token, &config).await;
    assert!(res.is_err(), "Should reject HS256 in strict mode");
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Algorithm HS256 not allowed"));
}

#[tokio::test]
async fn test_typ_enforcement() {
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("bad-typ".to_string());
    // Note: We can't easily sign RS256 here without a key, so this test relies on header parsing failing
    // OR we use a validator that fails early.
    // Our validator checks header *before* key resolution.

    // Token with forged header: {"typ":"bad-typ","alg":"RS256"}
    let part1 = URL_SAFE_NO_PAD.encode(r#"{"alg":"RS256","typ":"bad-typ"}"#);
    let token = format!("{}.e30.signature", part1);

    let validator = TokenValidator::new(None);
    let config = AuthConfig {
        mode: AuthMode::Strict,
        ..Default::default()
    };

    let _res = validator.validate(&token, &config).await;
    // It might fail on base64 first, but if it parses header, it should check typ
    // Let's ensure it fails on typ if header valid.

    // Better: use library to make token
    let mut header = Header::new(Algorithm::RS256);
    header.typ = Some("bad-type".to_string());
    // We use a dummy key just to form structure; validation fails before verify signature
    let _claims = valid_claims();
    // We sign with HS256 but claim RS256 in header? standard confusion attack.
    // But to test `typ` check specifically, we need `decode_header` to succeed.
    // `create_token` uses HS256.
    // Let's use `Algorithm::HS256` in header just to get a valid string, but validator checks alg whitelist.
    // To test `typ` check, we need to pass alg check? No, alg check is first.
    // So we need a valid alg (RS256) but invalid typ.
    // Since we don't have RS256 key loaded, it will fail on key lookup.
    // BUT we want to ensure `typ` check happens.
    // Our code: 1. Header Decode, 2. Alg Check, 3. Typ Check, 4. Key Lookup.
    // We can't strictly test Typ check without passing Alg check.
    // If we claim RS256, we pass Alg check. Then Typ Check.
    // So we can use any signature, as long as header is valid JSON.

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    let part1 = URL_SAFE_NO_PAD
        .encode(r#"{"alg":"RS256","typ":"bad"}"#)
        .replace("=", "");
    let token = format!("{}.{}.{}", part1, "e30", "sig");

    let res = validator.validate(&token, &config).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Token type 'bad' not accepted"));
}

#[tokio::test]
async fn test_header_hardening() {
    let validator = TokenValidator::new(None);
    let config = AuthConfig {
        mode: AuthMode::Strict,
        ..Default::default()
    };

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
    // Test JKU
    let part1 = URL_SAFE_NO_PAD
        .encode(r#"{"alg":"RS256","typ":"JWT","jku":"https://evil.com/keys"}"#)
        .replace("=", "");
    let token = format!("{}.{}.{}", part1, "e30", "sig");
    let res = validator.validate(&token, &config).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("dangerous headers"));

    // Test CRIT
    let part1 = URL_SAFE_NO_PAD
        .encode(r#"{"alg":"RS256","typ":"JWT","crit":["unknown"]}"#)
        .replace("=", "");
    let token = format!("{}.{}.{}", part1, "e30", "sig");
    let res = validator.validate(&token, &config).await;
    assert!(res.is_err());
    assert!(res.unwrap_err().to_string().contains("critical extensions"));
}

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
        // This might fail on building client or verify https in strict check if we test strict,
        // but JwksProvider::new only checks Scheme and IP.
        // It builds Client.
        let res = JwksProvider::new(url);
        assert!(res.is_ok(), "Should allow safe URI: {}", u);
    }
}

#[tokio::test]
async fn test_security_jwt_rs256_full_path() {
    // Generate transient test keys at runtime
    use rsa::{pkcs8::EncodePrivateKey, pkcs8::EncodePublicKey, RsaPrivateKey};
    let mut rng = rand::thread_rng();
    let bits = 2048;
    let priv_key = RsaPrivateKey::new(&mut rng, bits).expect("failed to generate key");
    let pub_key = priv_key.to_public_key();

    let priv_pem_str = priv_key
        .to_pkcs8_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap()
        .to_string();
    let pub_pem_str = pub_key
        .to_public_key_pem(rsa::pkcs8::LineEnding::LF)
        .unwrap()
        .to_string();

    let priv_pem = priv_pem_str.as_bytes();
    let pub_pem = pub_pem_str.as_bytes();

    let header = Header::new(Algorithm::RS256);
    let claims = valid_claims();

    let token = encode(
        &header,
        &claims,
        &EncodingKey::from_rsa_pem(priv_pem).unwrap(),
    )
    .unwrap();

    // Setup Validator with Mock JWKS containing our public key
    let validator = TokenValidator::new_with_static_key(pub_pem).unwrap();
    let config = AuthConfig {
        mode: AuthMode::Strict,
        audience: vec!["assay-mcp".to_string()],
        resource_id: Some("assay-system".to_string()),
        ..Default::default()
    };

    let res = validator.validate(&token, &config).await;
    assert!(
        res.is_ok(),
        "Should validate valid RS256 token: {:?}",
        res.err()
    );
    let claims_out = res.unwrap();
    assert_eq!(claims_out.sub, "user123");
    assert_eq!(
        claims_out.resource.as_ref().and_then(|v| v.as_str()),
        Some("assay-system")
    );
}

#[tokio::test]
async fn test_typ_enforcement_strict() {
    let validator = TokenValidator::new(None);
    let config = AuthConfig {
        mode: AuthMode::Strict,
        ..Default::default()
    };

    use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};

    // Case 1: Missing typ
    let part1 = URL_SAFE_NO_PAD
        .encode(r#"{"alg":"RS256"}"#)
        .replace("=", "");
    let token = format!("{}.e30.sig", part1);
    let res = validator.validate(&token, &config).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Missing 'typ' header"));

    // Case 2: Wrong typ
    let part1 = URL_SAFE_NO_PAD
        .encode(r#"{"alg":"RS256","typ":"text"}"#)
        .replace("=", "");
    let token = format!("{}.e30.sig", part1);
    let res = validator.validate(&token, &config).await;
    assert!(res.is_err());
    assert!(res
        .unwrap_err()
        .to_string()
        .contains("Token type 'text' not accepted"));
}
