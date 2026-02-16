use super::super::super::mandate_store::RevocationRecord;
use super::super::*;
use chrono::{DateTime, Duration, TimeZone, Utc};

fn test_config() -> AuthzConfig {
    AuthzConfig {
        clock_skew_seconds: 30,
        expected_audience: "org/app".to_string(),
        trusted_issuers: vec!["auth.org.com".to_string()],
    }
}

fn test_mandate() -> MandateData {
    MandateData {
        mandate_id: "sha256:test123".to_string(),
        mandate_kind: MandateKind::Intent,
        audience: "org/app".to_string(),
        issuer: "auth.org.com".to_string(),
        tool_patterns: vec!["search_*".to_string(), "get_*".to_string()],
        operation_class: Some(OperationClass::Read),
        transaction_ref: None,
        not_before: None,
        expires_at: Some(Utc::now() + Duration::hours(1)),
        single_use: false,
        max_uses: None,
        nonce: None,
        canonical_digest: "sha256:digest123".to_string(),
        key_id: "sha256:key123".to_string(),
    }
}

fn test_tool_call(name: &str) -> ToolCallData {
    ToolCallData {
        tool_call_id: format!("tc_{}", name),
        tool_name: name.to_string(),
        operation_class: OperationClass::Read,
        transaction_object: None,
        source_run_id: None,
    }
}

#[test]
fn test_glob_exact_match() {
    assert!(super::policy::glob_matches_impl("search", "search"));
    assert!(!super::policy::glob_matches_impl(
        "search",
        "search_products"
    ));
    assert!(!super::policy::glob_matches_impl("search", "my_search"));
}

#[test]
fn test_glob_single_star() {
    assert!(super::policy::glob_matches_impl(
        "search_*",
        "search_products"
    ));
    assert!(super::policy::glob_matches_impl("search_*", "search_users"));
    assert!(super::policy::glob_matches_impl("search_*", "search_"));
    assert!(!super::policy::glob_matches_impl(
        "search_*",
        "search.products"
    ));
}

#[test]
fn test_glob_double_star() {
    assert!(super::policy::glob_matches_impl("fs.**", "fs.read_file"));
    assert!(super::policy::glob_matches_impl(
        "fs.**",
        "fs.write.nested.path"
    ));
    assert!(super::policy::glob_matches_impl("**", "anything.at.all"));
}

#[test]
fn test_glob_escaped() {
    assert!(super::policy::glob_matches_impl(r"file\*name", "file*name"));
    assert!(!super::policy::glob_matches_impl(r"file\*name", "filename"));
}

fn fixed_now() -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000, 0).unwrap()
}

#[test]
fn test_authorize_rejects_expired() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);
    let now = fixed_now();

    let mut mandate = test_mandate();
    mandate.expires_at = Some(now - Duration::seconds(31));

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_at(now, &mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(PolicyError::Expired { .. }))
    ));
}

#[test]
fn test_authorize_allows_within_expiry_skew() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);
    let now = fixed_now();

    let mut mandate = test_mandate();
    mandate.expires_at = Some(now - Duration::seconds(5));

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_at(now, &mandate, &tool_call);

    assert!(result.is_ok());
}

#[test]
fn test_authorize_rejects_not_yet_valid() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);
    let now = fixed_now();

    let mut mandate = test_mandate();
    mandate.not_before = Some(now + Duration::seconds(31));

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_at(now, &mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(PolicyError::NotYetValid { .. }))
    ));
}

#[test]
fn test_authorize_rejects_tool_not_in_scope() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mandate = test_mandate();
    let tool_call = test_tool_call("purchase_item");

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(PolicyError::ToolNotInScope { tool })) if tool == "purchase_item"
    ));
}

#[test]
fn test_authorize_allows_tool_in_scope() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mandate = test_mandate();
    let tool_call = test_tool_call("search_products");

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);
    assert!(result.is_ok());
}

#[test]
fn test_authorize_rejects_commit_with_intent_mandate() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mut mandate = test_mandate();
    mandate.mandate_kind = MandateKind::Intent;
    mandate.tool_patterns = vec!["purchase_*".to_string()];

    let mut tool_call = test_tool_call("purchase_item");
    tool_call.operation_class = OperationClass::Commit;

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(PolicyError::KindMismatch { .. }))
    ));
}

#[test]
fn test_authorize_allows_commit_with_transaction_mandate() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mut mandate = test_mandate();
    mandate.mandate_kind = MandateKind::Transaction;
    mandate.tool_patterns = vec!["purchase_*".to_string()];

    let mut tool_call = test_tool_call("purchase_item");
    tool_call.operation_class = OperationClass::Commit;

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);
    assert!(result.is_ok());
}

#[test]
fn test_authorize_rejects_missing_transaction_object() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mut mandate = test_mandate();
    mandate.mandate_kind = MandateKind::Transaction;
    mandate.tool_patterns = vec!["purchase_*".to_string()];
    mandate.transaction_ref = Some("sha256:expected".to_string());

    let mut tool_call = test_tool_call("purchase_item");
    tool_call.operation_class = OperationClass::Commit;
    tool_call.transaction_object = None;

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(
            PolicyError::MissingTransactionObject
        ))
    ));
}

#[test]
fn test_authorize_rejects_transaction_ref_mismatch() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let expected_obj = serde_json::json!({
        "merchant_id": "shop_123",
        "amount_cents": 4999,
        "currency": "EUR"
    });
    let expected_ref = super::policy::compute_transaction_ref_impl(&expected_obj).unwrap();

    let mut mandate = test_mandate();
    mandate.mandate_kind = MandateKind::Transaction;
    mandate.tool_patterns = vec!["purchase_*".to_string()];
    mandate.transaction_ref = Some(expected_ref);

    let mut tool_call = test_tool_call("purchase_item");
    tool_call.operation_class = OperationClass::Commit;
    tool_call.transaction_object = Some(serde_json::json!({
        "merchant_id": "shop_123",
        "amount_cents": 9999,
        "currency": "EUR"
    }));

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(
            PolicyError::TransactionRefMismatch { .. }
        ))
    ));
}

#[test]
fn test_authorize_allows_matching_transaction_ref() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let tx_obj = serde_json::json!({
        "merchant_id": "shop_123",
        "amount_cents": 4999,
        "currency": "EUR"
    });
    let tx_ref = super::policy::compute_transaction_ref_impl(&tx_obj).unwrap();

    let mut mandate = test_mandate();
    mandate.mandate_kind = MandateKind::Transaction;
    mandate.tool_patterns = vec!["purchase_*".to_string()];
    mandate.transaction_ref = Some(tx_ref);

    let mut tool_call = test_tool_call("purchase_item");
    tool_call.operation_class = OperationClass::Commit;
    tool_call.transaction_object = Some(tx_obj);

    let result = authorizer.authorize_and_consume(&mandate, &tool_call);
    assert!(result.is_ok());
}

#[test]
fn test_authorize_rejects_wrong_audience() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mut mandate = test_mandate();
    mandate.audience = "other/app".to_string();

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(PolicyError::AudienceMismatch { .. }))
    ));
}

#[test]
fn test_authorize_rejects_untrusted_issuer() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store, config);

    let mut mandate = test_mandate();
    mandate.issuer = "evil.attacker.com".to_string();

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(matches!(
        result,
        Err(AuthorizeError::Policy(PolicyError::IssuerNotTrusted { .. }))
    ));
}

#[test]
fn test_authorize_rejects_revoked_mandate() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store.clone(), config);

    let mandate = test_mandate();

    store
        .upsert_revocation(&RevocationRecord {
            mandate_id: mandate.mandate_id.clone(),
            revoked_at: Utc::now() - chrono::Duration::minutes(5),
            reason: Some("User requested".to_string()),
            revoked_by: None,
            source: None,
            event_id: None,
        })
        .unwrap();

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(
        matches!(
            result,
            Err(AuthorizeError::Store(AuthzError::Revoked { .. }))
        ),
        "Expected Revoked error, got {:?}",
        result
    );
}

#[test]
fn test_authorize_allows_if_revoked_in_future() {
    let store = MandateStore::memory().unwrap();
    let config = test_config();
    let authorizer = Authorizer::new(store.clone(), config);

    let mandate = test_mandate();

    store
        .upsert_revocation(&RevocationRecord {
            mandate_id: mandate.mandate_id.clone(),
            revoked_at: Utc::now() + chrono::Duration::hours(1),
            reason: Some("Scheduled revocation".to_string()),
            revoked_by: None,
            source: None,
            event_id: None,
        })
        .unwrap();

    let tool_call = test_tool_call("search_products");
    let result = authorizer.authorize_and_consume(&mandate, &tool_call);

    assert!(result.is_ok(), "Should allow use before revoked_at");
}
