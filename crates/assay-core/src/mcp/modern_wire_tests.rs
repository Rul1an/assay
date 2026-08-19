use super::modern_wire::{
    validate_cacheable_result, validate_discover_request, validate_discover_result,
    validate_request_metadata, validate_unsupported_protocol_version_error, ModernWireError,
    MODERN_PROTOCOL_VERSION,
};
use serde_json::json;

fn modern_request() -> serde_json::Value {
    json!({
        "params": {
            "_meta": {
                "io.modelcontextprotocol/protocolVersion": MODERN_PROTOCOL_VERSION,
                "io.modelcontextprotocol/clientCapabilities": {}
            }
        }
    })
}

#[test]
fn a_modern_request_requires_the_protocol_version_metadata_member() {
    let mut request = modern_request();
    request["params"]["_meta"]
        .as_object_mut()
        .expect("metadata object")
        .remove("io.modelcontextprotocol/protocolVersion");

    assert_eq!(
        validate_request_metadata(&request),
        Err(ModernWireError::MissingProtocolVersion)
    );
}

#[test]
fn a_modern_request_requires_the_client_capabilities_metadata_member() {
    let mut request = modern_request();
    request["params"]["_meta"]
        .as_object_mut()
        .expect("metadata object")
        .remove("io.modelcontextprotocol/clientCapabilities");

    assert_eq!(
        validate_request_metadata(&request),
        Err(ModernWireError::MissingClientCapabilities)
    );
}

#[test]
fn a_modern_request_does_not_require_optional_client_info() {
    assert_eq!(validate_request_metadata(&modern_request()), Ok(()));
}

#[test]
fn a_cacheable_result_requires_each_schema_member() {
    let valid = json!({
        "resultType": "complete",
        "ttlMs": 0,
        "cacheScope": "private"
    });

    for (member, expected) in [
        ("resultType", ModernWireError::MissingResultType),
        ("ttlMs", ModernWireError::MissingTtlMs),
        ("cacheScope", ModernWireError::MissingCacheScope),
    ] {
        let mut result = valid.clone();
        result
            .as_object_mut()
            .expect("result object")
            .remove(member);
        assert_eq!(
            validate_cacheable_result(&result),
            Err(expected),
            "{member}"
        );
    }
}

#[test]
fn a_cacheable_result_accepts_only_the_pinned_cache_hint_shape() {
    assert_eq!(
        validate_cacheable_result(&json!({
            "resultType": "complete",
            "ttlMs": 1,
            "cacheScope": "public"
        })),
        Ok(())
    );
    assert_eq!(
        validate_cacheable_result(&json!({
            "resultType": "complete",
            "ttlMs": -1,
            "cacheScope": "public"
        })),
        Err(ModernWireError::MalformedTtlMs)
    );
    assert_eq!(
        validate_cacheable_result(&json!({
            "resultType": "complete",
            "ttlMs": 1,
            "cacheScope": "shared"
        })),
        Err(ModernWireError::MalformedCacheScope)
    );
}

#[test]
fn a_discover_request_has_a_modern_request_shape() {
    let request = json!({
        "jsonrpc": "2.0",
        "id": "discover-1",
        "method": "server/discover",
        "params": modern_request()["params"].clone()
    });
    assert_eq!(validate_discover_request(&request), Ok(()));

    for member in ["id", "jsonrpc", "method", "params"] {
        let mut invalid = request.clone();
        invalid
            .as_object_mut()
            .expect("request object")
            .remove(member);
        assert_eq!(
            validate_discover_request(&invalid),
            Err(ModernWireError::MalformedDiscoverRequest),
            "{member}"
        );
    }
}

#[test]
fn a_discover_result_requires_each_schema_member() {
    let valid = json!({
        "resultType": "complete",
        "ttlMs": 0,
        "cacheScope": "private",
        "capabilities": {},
        "supportedVersions": [MODERN_PROTOCOL_VERSION]
    });
    assert_eq!(validate_discover_result(&valid), Ok(()));

    for member in [
        "resultType",
        "ttlMs",
        "cacheScope",
        "capabilities",
        "supportedVersions",
    ] {
        let mut invalid = valid.clone();
        invalid
            .as_object_mut()
            .expect("result object")
            .remove(member);
        assert_eq!(
            validate_discover_result(&invalid),
            Err(ModernWireError::MalformedDiscoverResult),
            "{member}"
        );
    }
}

#[test]
fn an_unsupported_protocol_version_error_requires_requested_and_supported() {
    let valid = json!({
        "jsonrpc": "2.0",
        "error": {
            "code": -32022,
            "message": "Unsupported protocol version",
            "data": {
                "requested": MODERN_PROTOCOL_VERSION,
                "supported": ["2025-11-25"]
            }
        }
    });
    assert_eq!(validate_unsupported_protocol_version_error(&valid), Ok(()));

    for member in ["requested", "supported"] {
        let mut invalid = valid.clone();
        invalid["error"]["data"]
            .as_object_mut()
            .expect("error data")
            .remove(member);
        assert_eq!(
            validate_unsupported_protocol_version_error(&invalid),
            Err(ModernWireError::MalformedUnsupportedProtocolVersionError),
            "{member}"
        );
    }

    let mut invalid_id = valid;
    invalid_id["id"] = json!({"not": "a request id"});
    assert_eq!(
        validate_unsupported_protocol_version_error(&invalid_id),
        Err(ModernWireError::MalformedUnsupportedProtocolVersionError)
    );
}
