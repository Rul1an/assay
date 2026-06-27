use base64::{engine::general_purpose::STANDARD as BASE64, Engine};

use super::body::HashedRekordBody;
use super::checkpoint::{parse_checkpoint, rfc6962_root};
use super::trusted_root::normalize_origin;

const VALID_BODY: &str = r#"{"apiVersion":"0.0.2","kind":"hashedrekord","spec":{"hashedRekordV002":{"data":{"algorithm":"SHA2_256","digest":"AA=="},"signature":{"content":"AA==","verifier":{"keyDetails":"PKIX_ECDSA_P256_SHA_256","x509Certificate":{"rawBytes":"AA=="}}}}}}"#;

fn parses(body: &str) -> bool {
    serde_json::from_str::<HashedRekordBody>(body).is_ok()
}

#[test]
fn strict_body_schema_accepts_supported_shape() {
    assert!(parses(VALID_BODY));
}

#[test]
fn strict_body_schema_rejects_unknown_top_level_field() {
    let body = VALID_BODY.replacen(
        r#""kind":"hashedrekord""#,
        r#""kind":"hashedrekord","extra":"x""#,
        1,
    );
    assert!(
        !parses(&body),
        "deny_unknown_fields must reject extra top-level field"
    );
}

#[test]
fn strict_body_schema_rejects_unknown_nested_field() {
    let body = VALID_BODY.replacen(
        r#""algorithm":"SHA2_256""#,
        r#""algorithm":"SHA2_256","rogue":"x""#,
        1,
    );
    assert!(
        !parses(&body),
        "deny_unknown_fields must reject extra nested field"
    );
}

#[test]
fn strict_body_schema_rejects_missing_required_field() {
    let body = VALID_BODY.replacen(r#","digest":"AA=="#, "", 1);
    assert!(!parses(&body), "missing required field must not parse");
}

#[test]
fn strict_body_schema_rejects_duplicate_field() {
    // serde's derived struct deserializer rejects a duplicate field.
    let body = VALID_BODY.replacen(
        r#""kind":"hashedrekord""#,
        r#""kind":"hashedrekord","kind":"x""#,
        1,
    );
    assert!(!parses(&body), "duplicate field must not parse");
}

fn shape_ok(body: &str) -> bool {
    serde_json::from_str::<HashedRekordBody>(body)
        .map(|b| b.shape_supported())
        .unwrap_or(false)
}

#[test]
fn shape_check_accepts_supported_then_rejects_wrong_values() {
    assert!(shape_ok(VALID_BODY));
    assert!(!shape_ok(&VALID_BODY.replacen(
        r#""apiVersion":"0.0.2""#,
        r#""apiVersion":"0.0.1""#,
        1
    )));
    assert!(!shape_ok(&VALID_BODY.replacen(
        r#""kind":"hashedrekord""#,
        r#""kind":"dsse""#,
        1
    )));
    assert!(!shape_ok(&VALID_BODY.replacen(
        r#""algorithm":"SHA2_256""#,
        r#""algorithm":"SHA2_512""#,
        1
    )));
}

#[test]
fn normalize_origin_strips_scheme_and_trailing_slash() {
    assert_eq!(
        normalize_origin("https://log.example.dev/"),
        "log.example.dev"
    );
    assert_eq!(
        normalize_origin("http://log.example.dev"),
        "log.example.dev"
    );
    assert_eq!(normalize_origin("log.example.dev"), "log.example.dev");
}

#[test]
fn rfc6962_rejects_out_of_range_index() {
    // leaf index >= tree size is impossible -> None.
    assert!(rfc6962_root([0u8; 32], 5, 5, &[]).is_none());
}

#[test]
fn checkpoint_signed_text_includes_extension_lines() {
    // Regression (carried from a-3.3c): a C2SP checkpoint may carry optional extension lines after the
    // root-hash line. parse_checkpoint must extract the fields from the first three lines yet PRESERVE
    // the extension lines in `signed_text`, because the checkpoint signature covers the whole note text
    // (everything up to the blank line). Truncating the signed body at the root hash would silently
    // break signature verification for any real checkpoint that uses extensions.
    use ed25519_dalek::ed25519::signature::Signer;
    use ed25519_dalek::SigningKey;

    let origin = "rekor.example.dev";
    let tree_size = 42u64;
    let root_hash = [0x11u8; 32];
    let b64root = BASE64.encode(root_hash);
    // Note text = origin / tree_size / base64(root) / two extension lines, each terminated by '\n'.
    let text = format!(
        "{origin}\n{tree_size}\n{b64root}\nTimestamp: 1700000000\nVendor: assay-regression\n"
    );

    // Sign the FULL note text (extensions included), as a real log signs the checkpoint.
    let sk = SigningKey::from_bytes(&[7u8; 32]);
    let vk = sk.verifying_key();
    let sig = sk.sign(text.as_bytes());

    // Envelope = text, a blank line, then a C2SP signature line.
    let mut hinted = vec![0xAA, 0xBB, 0xCC, 0xDD];
    hinted.extend_from_slice(&sig.to_bytes());
    let envelope = format!("{text}\n\u{2014} {origin} {}\n", BASE64.encode(&hinted));

    let cp = parse_checkpoint(&envelope).expect("checkpoint with extension lines must parse");

    // Fields come from the first three lines; the extension lines do not disturb them.
    assert_eq!(cp.origin, origin);
    assert_eq!(cp.tree_size, tree_size);
    assert_eq!(cp.root_hash, root_hash.to_vec());

    // The signed text preserves the extension lines verbatim (up to and including the newline before
    // the blank line), so the signature over the full text verifies.
    assert_eq!(cp.signed_text, text.as_bytes());
    assert!(vk.verify_strict(&cp.signed_text, &sig).is_ok());

    // A signature computed over only the 3-line body (extensions dropped) must NOT verify against the
    // preserved signed text; this is precisely what the regression guards against.
    let truncated = format!("{origin}\n{tree_size}\n{b64root}\n");
    let sig_trunc = sk.sign(truncated.as_bytes());
    assert!(vk.verify_strict(&cp.signed_text, &sig_trunc).is_err());
}
