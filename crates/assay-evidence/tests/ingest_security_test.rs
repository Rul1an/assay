//! Ingest security tests for hostile JSON input.
//!
//! Tests that the strict JSON parser rejects:
//! - Duplicate keys (semantic divergence attack)
//! - Lone surrogates (verification bypass)

use assay_evidence::json_strict::validate_json_strict;
use assay_evidence::ndjson::NdjsonEvents;
use std::io::{BufReader, Cursor};

/// Test that duplicate keys in JSON are rejected at ingest.
#[test]
fn test_ingest_rejects_duplicate_keys() {
    // Attack: duplicate mandate_id to confuse verification
    let hostile_json =
        r#"{"mandate_id":"sha256:legit","mandate_id":"sha256:evil","kind":"intent"}"#;

    let result = validate_json_strict(hostile_json);
    assert!(
        result.is_err(),
        "Duplicate keys MUST be rejected: {:?}",
        result
    );

    let err = result.unwrap_err();
    assert!(
        err.to_string().contains("Duplicate key"),
        "Error must mention duplicate key: {}",
        err
    );
}

/// Test that nested duplicate keys are rejected.
#[test]
fn test_ingest_rejects_nested_duplicate_keys() {
    // Attack: duplicate key in nested object (3 levels deep)
    let hostile_json = r#"{"data":{"scope":{"tools":["a"],"tools":["b"]}}}"#;

    let result = validate_json_strict(hostile_json);
    assert!(
        result.is_err(),
        "Nested duplicate keys MUST be rejected: {:?}",
        result
    );
}

/// Test that signature object duplicate keys are rejected.
#[test]
fn test_ingest_rejects_signature_duplicate_key() {
    // Attack: duplicate key_id in signature
    let hostile_json =
        r#"{"signature":{"key_id":"sha256:legit","key_id":"sha256:evil","algorithm":"ed25519"}}"#;

    let result = validate_json_strict(hostile_json);
    assert!(result.is_err(), "Signature duplicate keys MUST be rejected");
}

/// Test that lone high surrogate is rejected.
#[test]
fn test_ingest_rejects_lone_high_surrogate() {
    // Attack: lone high surrogate could cause verification mismatch
    let hostile_json = r#"{"value":"\uD800"}"#;

    let result = validate_json_strict(hostile_json);
    assert!(
        result.is_err(),
        "Lone high surrogate MUST be rejected: {:?}",
        result
    );

    let err = result.unwrap_err();
    assert!(
        err.to_string().to_lowercase().contains("surrogate"),
        "Error must mention surrogate: {}",
        err
    );
}

/// Test that lone low surrogate is rejected.
#[test]
fn test_ingest_rejects_lone_low_surrogate() {
    // Attack: lone low surrogate
    let hostile_json = r#"{"value":"\uDC00"}"#;

    let result = validate_json_strict(hostile_json);
    assert!(
        result.is_err(),
        "Lone low surrogate MUST be rejected: {:?}",
        result
    );
}

/// Test that reversed surrogate pair is rejected.
#[test]
fn test_ingest_rejects_reversed_surrogate_pair() {
    // Attack: low surrogate followed by high (wrong order)
    let hostile_json = r#"{"value":"\uDC00\uD800"}"#;

    let result = validate_json_strict(hostile_json);
    assert!(result.is_err(), "Reversed surrogate pair MUST be rejected");
}

/// Test that valid surrogate pair is accepted.
#[test]
fn test_ingest_accepts_valid_surrogate_pair() {
    // Valid: high + low surrogate = 😀
    let valid_json = r#"{"value":"\uD83D\uDE00"}"#;

    let result = validate_json_strict(valid_json);
    assert!(
        result.is_ok(),
        "Valid surrogate pair should be accepted: {:?}",
        result
    );
}

/// Test NDJSON ingest fails on duplicate keys in event.
#[test]
fn test_ndjson_ingest_rejects_duplicate_keys() {
    let ndjson = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"key":"a","key":"b"}}"#;

    let cursor = Cursor::new(ndjson);
    let reader = BufReader::new(cursor);
    let mut iter = NdjsonEvents::new(reader);

    let result = iter.next().unwrap();
    assert!(result.is_err(), "NDJSON ingest MUST fail on duplicate keys");

    let err = result.unwrap_err().to_string();
    assert!(
        err.to_lowercase().contains("duplicate") || err.contains("strict validation"),
        "Error should mention duplicate key issue: {}",
        err
    );
}

/// Test NDJSON ingest fails on lone surrogate.
#[test]
fn test_ndjson_ingest_rejects_lone_surrogate() {
    let ndjson = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"value":"\uD800"}}"#;

    let cursor = Cursor::new(ndjson);
    let reader = BufReader::new(cursor);
    let mut iter = NdjsonEvents::new(reader);

    let result = iter.next().unwrap();
    assert!(result.is_err(), "NDJSON ingest MUST fail on lone surrogate");
}

/// Test that valid NDJSON passes strict validation.
#[test]
fn test_ndjson_ingest_accepts_valid_json() {
    let ndjson = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2023-11-14T22:13:20Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"valid":"json"}}"#;

    let cursor = Cursor::new(ndjson);
    let reader = BufReader::new(cursor);
    let events: Vec<_> = NdjsonEvents::new(reader).collect();

    assert_eq!(events.len(), 1);
    assert!(events[0].is_ok(), "Valid JSON should be accepted");
}
