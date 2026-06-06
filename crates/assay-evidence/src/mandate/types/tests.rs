use super::*;
use chrono::Utc;

#[test]
fn test_operation_class_ordering() {
    assert!(OperationClass::Read < OperationClass::Write);
    assert!(OperationClass::Write < OperationClass::Commit);
    assert!(OperationClass::Read < OperationClass::Commit);
}

#[test]
fn test_operation_class_allows() {
    assert!(OperationClass::Commit.allows(OperationClass::Read));
    assert!(OperationClass::Commit.allows(OperationClass::Write));
    assert!(OperationClass::Commit.allows(OperationClass::Commit));

    assert!(OperationClass::Write.allows(OperationClass::Read));
    assert!(OperationClass::Write.allows(OperationClass::Write));
    assert!(!OperationClass::Write.allows(OperationClass::Commit));

    assert!(OperationClass::Read.allows(OperationClass::Read));
    assert!(!OperationClass::Read.allows(OperationClass::Write));
    assert!(!OperationClass::Read.allows(OperationClass::Commit));
}

#[test]
fn test_validity_check() {
    use chrono::TimeZone;

    let now = Utc.with_ymd_and_hms(2026, 1, 28, 12, 0, 0).unwrap();
    let before = Utc.with_ymd_and_hms(2026, 1, 28, 11, 0, 0).unwrap();
    let after = Utc.with_ymd_and_hms(2026, 1, 28, 13, 0, 0).unwrap();

    // No constraints
    let validity = Validity::at(before);
    assert!(validity.is_valid_at(now));

    // Valid window
    let validity = Validity::at(before)
        .with_not_before(before)
        .with_expires_at(after);
    assert!(validity.is_valid_at(now));

    // Not yet valid
    let validity = Validity::at(now).with_not_before(after);
    assert!(!validity.is_valid_at(now));

    // Expired
    let validity = Validity::at(before).with_expires_at(before);
    assert!(!validity.is_valid_at(now));
}

#[test]
fn test_constraints_single_use() {
    let constraints = Constraints::single_use();
    assert!(constraints.single_use);
    assert_eq!(constraints.effective_max_uses(), Some(1));

    assert!(constraints.is_use_allowed(0));
    assert!(!constraints.is_use_allowed(1));
}

#[test]
fn test_constraints_max_uses() {
    let constraints = Constraints::unlimited().with_max_uses(3);
    assert_eq!(constraints.effective_max_uses(), Some(3));

    assert!(constraints.is_use_allowed(0));
    assert!(constraints.is_use_allowed(1));
    assert!(constraints.is_use_allowed(2));
    assert!(!constraints.is_use_allowed(3));
}

#[test]
fn test_mandate_kind_serialization() {
    assert_eq!(
        serde_json::to_string(&MandateKind::Intent).unwrap(),
        "\"intent\""
    );
    assert_eq!(
        serde_json::to_string(&MandateKind::Transaction).unwrap(),
        "\"transaction\""
    );
}

#[test]
fn test_operation_class_serialization() {
    assert_eq!(
        serde_json::to_string(&OperationClass::Read).unwrap(),
        "\"read\""
    );
    assert_eq!(
        serde_json::to_string(&OperationClass::Write).unwrap(),
        "\"write\""
    );
    assert_eq!(
        serde_json::to_string(&OperationClass::Commit).unwrap(),
        "\"commit\""
    );
}

#[test]
fn test_mandate_content_serialization_shape() {
    use chrono::TimeZone;

    let issued_at = Utc.with_ymd_and_hms(2026, 1, 28, 10, 0, 0).unwrap();
    let not_before = Utc.with_ymd_and_hms(2026, 1, 28, 10, 5, 0).unwrap();
    let expires_at = Utc.with_ymd_and_hms(2026, 1, 28, 11, 0, 0).unwrap();

    let content = MandateContent {
        mandate_kind: MandateKind::Transaction,
        principal: Principal::new("sub_opaque_123", AuthMethod::Oidc)
            .with_display("Example User")
            .with_credential_ref("sha256:abc123"),
        scope: Scope::new(vec!["payments.*".to_string()])
            .with_resources(vec!["acct:checking".to_string()])
            .with_operation_class(OperationClass::Commit)
            .with_max_value(MaxValue::new("42.00", "EUR"))
            .with_transaction_ref("sha256:cart123"),
        validity: Validity::at(issued_at)
            .with_not_before(not_before)
            .with_expires_at(expires_at),
        constraints: Constraints::single_use().with_require_confirmation(),
        context: Context::new("myorg/app/prod", "auth.myorg.com")
            .with_nonce("nonce-128-bit-example")
            .with_traceparent("00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"),
    };

    let actual = serde_json::to_value(&content).unwrap();

    assert_eq!(
        actual,
        serde_json::json!({
            "mandate_kind": "transaction",
            "principal": {
                "subject": "sub_opaque_123",
                "method": "oidc",
                "display": "Example User",
                "credential_ref": "sha256:abc123"
            },
            "scope": {
                "tools": ["payments.*"],
                "resources": ["acct:checking"],
                "operation_class": "commit",
                "max_value": {
                    "amount": "42.00",
                    "currency": "EUR"
                },
                "transaction_ref": "sha256:cart123"
            },
            "validity": {
                "issued_at": "2026-01-28T10:00:00Z",
                "not_before": "2026-01-28T10:05:00Z",
                "expires_at": "2026-01-28T11:00:00Z"
            },
            "constraints": {
                "single_use": true,
                "max_uses": 1,
                "require_confirmation": true
            },
            "context": {
                "audience": "myorg/app/prod",
                "issuer": "auth.myorg.com",
                "nonce": "nonce-128-bit-example",
                "traceparent": "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-00"
            }
        })
    );
}

#[test]
fn test_mandate_builder() {
    let content = Mandate::builder()
        .kind(MandateKind::Intent)
        .principal(Principal::new("user-123", AuthMethod::Oidc))
        .scope(Scope::new(vec!["search_*".to_string()]))
        .validity(Validity::now())
        .context(Context::new("myorg/app", "auth.myorg.com"))
        .build()
        .unwrap();

    assert_eq!(content.mandate_kind, MandateKind::Intent);
    assert_eq!(content.principal.subject, "user-123");
}
