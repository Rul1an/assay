use super::*;

#[test]
fn test_verify_pack_fail_closed_matrix_contract() {
    let trust_store = TrustStore::new();
    let content = "name: test-pack\nversion: \"1.0.0\"\nrules: []";
    let digest = compute_digest(content);

    let unsigned = make_fetch_result(content, Some(digest.clone()), None, None);

    // Unsigned is rejected by default.
    let err_unsigned_default = verify_pack(&unsigned, &trust_store, &VerifyOptions::default())
        .expect_err("unsigned pack must fail closed by default");
    assert!(matches!(
        err_unsigned_default,
        RegistryError::Unsigned { .. }
    ));

    // Unsigned may pass only when explicitly allowed.
    let allowed = verify_pack(
        &unsigned,
        &trust_store,
        &VerifyOptions::default().allow_unsigned(),
    )
    .expect("allow_unsigned should permit unsigned input");
    assert!(!allowed.signed);
    assert!(allowed.key_id.is_none());
    assert_eq!(allowed.digest, digest);

    // Digest mismatch must still fail closed even when allow_unsigned is enabled.
    let mismatch = make_fetch_result(
        content,
        Some("sha256:0000000000000000000000000000000000000000000000000000000000000000".to_string()),
        None,
        None,
    );
    let err_mismatch = verify_pack(
        &mismatch,
        &trust_store,
        &VerifyOptions::default().allow_unsigned(),
    )
    .expect_err("digest mismatch must fail closed before signature policy");
    assert!(matches!(err_mismatch, RegistryError::DigestMismatch { .. }));
}

#[test]
fn test_verify_pack_malformed_signature_reason_is_stable() {
    let trust_store = TrustStore::new();
    let content = "name: malformed-signature\nversion: \"1.0.0\"";
    let digest = compute_digest(content);
    let malformed = make_fetch_result(
        content,
        Some(digest),
        Some("not base64 envelope".to_string()),
        None,
    );

    let err = verify_pack(&malformed, &trust_store, &VerifyOptions::default())
        .expect_err("malformed signature header must fail closed");
    match err {
        RegistryError::SignatureInvalid { reason } => {
            assert!(
                reason.starts_with("invalid base64 envelope:"),
                "reason prefix drifted: {reason}"
            );
        }
        other => panic!("expected SignatureInvalid for malformed signature, got {other:?}"),
    }
}
