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
