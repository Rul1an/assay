//! Bundle-level security integration tests.
//!
//! Tests that strict JSON validation propagates through the entire
//! ingest pipeline (NDJSON reader used by bundle verification).

use assay_evidence::bundle::writer::{ErrorClass, ErrorCode, VerifyError};
use assay_evidence::ndjson::NdjsonEvents;
use std::io::{BufReader, Cursor};

/// Integration test: NDJSON reader rejects duplicate keys in event.
///
/// This tests the same code path used by verify_bundle() for events.
#[test]
fn test_ndjson_ingest_rejects_duplicate_keys() {
    // Event with duplicate "type" key - security attack vector
    let evil_event = r#"{"specversion":"1.0","type":"assay.test","type":"assay.attack","source":"urn:assay:test","id":"run:0","time":"2026-01-28T10:00:00Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{}}"#;

    let cursor = Cursor::new(evil_event);
    let reader = BufReader::new(cursor);
    let mut iter = NdjsonEvents::new(reader);

    let result = iter.next().unwrap();
    assert!(
        result.is_err(),
        "NDJSON with duplicate keys MUST be rejected"
    );

    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("strict validation") || err_str.contains("duplicate"),
        "Error should mention strict validation or duplicate: {}",
        err_str
    );

    println!("✅ NDJSON ingest correctly rejected duplicate keys");
    println!("   Error: {}", err_str);
}

/// Integration test: NDJSON reader rejects lone surrogate.
#[test]
fn test_ndjson_ingest_rejects_lone_surrogate() {
    // Event with lone high surrogate - verification bypass attack
    let evil_event = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2026-01-28T10:00:00Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"evil":"\uD800"}}"#;

    let cursor = Cursor::new(evil_event);
    let reader = BufReader::new(cursor);
    let mut iter = NdjsonEvents::new(reader);

    let result = iter.next().unwrap();
    assert!(
        result.is_err(),
        "NDJSON with lone surrogate MUST be rejected"
    );

    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.contains("strict validation") || err_str.contains("surrogate"),
        "Error should mention strict validation or surrogate: {}",
        err_str
    );

    println!("✅ NDJSON ingest correctly rejected lone surrogate");
    println!("   Error: {}", err_str);
}

/// Integration test: NDJSON reader rejects unicode-escaped duplicate key.
#[test]
fn test_ndjson_ingest_rejects_unicode_escape_duplicate() {
    // "data" appears twice: once literally, once as \u0064\u0061\u0074\u0061
    let evil_event = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2026-01-28T10:00:00Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{},"d\u0061ta":{}}"#;

    let cursor = Cursor::new(evil_event);
    let reader = BufReader::new(cursor);
    let mut iter = NdjsonEvents::new(reader);

    let result = iter.next().unwrap();
    assert!(
        result.is_err(),
        "NDJSON with unicode-escaped duplicate MUST be rejected"
    );

    println!("✅ NDJSON ingest correctly rejected unicode-escaped duplicate");
}

/// Integration test: NDJSON reader rejects nested duplicate keys.
#[test]
fn test_ndjson_ingest_rejects_nested_duplicate() {
    // Duplicate key in nested "data" object
    let evil_event = r#"{"specversion":"1.0","type":"assay.test","source":"urn:assay:test","id":"run:0","time":"2026-01-28T10:00:00Z","datacontenttype":"application/json","assayrunid":"run","assayseq":0,"assayproducer":"test","assayproducerversion":"1.0","assaygit":"abc","assaypii":false,"assaysecrets":false,"data":{"key":"a","key":"b"}}"#;

    let cursor = Cursor::new(evil_event);
    let reader = BufReader::new(cursor);
    let mut iter = NdjsonEvents::new(reader);

    let result = iter.next().unwrap();
    assert!(
        result.is_err(),
        "NDJSON with nested duplicate MUST be rejected"
    );

    let err_str = result.unwrap_err().to_string();
    assert!(
        err_str.to_lowercase().contains("duplicate") && err_str.contains("/data"),
        "Error should mention duplicate at /data path: {}",
        err_str
    );

    println!("✅ NDJSON ingest correctly rejected nested duplicate");
    println!("   Error: {}", err_str);
}

/// Test error class/code mapping for security violations.
#[test]
fn test_error_taxonomy() {
    // Verify error taxonomy is consistent
    assert_eq!(
        format!("{:?}", ErrorClass::Contract),
        "Contract",
        "Contract class should stringify correctly"
    );
    assert_eq!(
        format!("{:?}", ErrorCode::ContractInvalidJson),
        "ContractInvalidJson",
        "ContractInvalidJson code should stringify correctly"
    );

    // Verify VerifyError can be constructed with security context
    let err = VerifyError::new(
        ErrorClass::Security,
        ErrorCode::SecurityPathTraversal,
        "Test path traversal",
    );
    assert_eq!(err.class, ErrorClass::Security);
    assert_eq!(err.code, ErrorCode::SecurityPathTraversal);

    // Verify error codes map to classes
    let json_err = VerifyError::new(
        ErrorClass::Contract,
        ErrorCode::ContractInvalidJson,
        "Security: Duplicate key 'type' at path '/'",
    );
    assert_eq!(json_err.class, ErrorClass::Contract);
    assert_eq!(json_err.code, ErrorCode::ContractInvalidJson);

    println!("✅ Error taxonomy verified");
    println!("   Classes: Integrity, Contract, Security, Limits");
    println!("   JSON errors use: Contract/ContractInvalidJson");
}
