use super::super::super::mandate_store::{ConsumeParams, MandateMetadata};
use super::super::{
    AuthorizeError, Authorizer, AuthzError, AuthzReceipt, MandateData, ToolCallData,
};
use chrono::{DateTime, Utc};

pub(super) fn check_revocation_impl(
    authorizer: &Authorizer,
    now: DateTime<Utc>,
    mandate: &MandateData,
) -> Result<(), AuthorizeError> {
    if let Some(revoked_at) = authorizer.store.get_revoked_at(&mandate.mandate_id)? {
        if now >= revoked_at {
            return Err(AuthzError::Revoked { revoked_at }.into());
        }
    }
    Ok(())
}

pub(super) fn upsert_mandate_metadata_impl(
    authorizer: &Authorizer,
    mandate: &MandateData,
) -> Result<(), AuthorizeError> {
    let meta = MandateMetadata {
        mandate_id: mandate.mandate_id.clone(),
        mandate_kind: mandate.mandate_kind.as_str().to_string(),
        audience: mandate.audience.clone(),
        issuer: mandate.issuer.clone(),
        expires_at: mandate.expires_at,
        single_use: mandate.single_use,
        max_uses: mandate.max_uses,
        canonical_digest: mandate.canonical_digest.clone(),
        key_id: mandate.key_id.clone(),
    };
    authorizer.store.upsert_mandate(&meta)?;
    Ok(())
}

pub(super) fn consume_mandate_impl(
    authorizer: &Authorizer,
    mandate: &MandateData,
    tool_call: &ToolCallData,
) -> Result<AuthzReceipt, AuthorizeError> {
    let receipt = authorizer.store.consume_mandate(&ConsumeParams {
        mandate_id: &mandate.mandate_id,
        tool_call_id: &tool_call.tool_call_id,
        nonce: mandate.nonce.as_deref(),
        audience: &mandate.audience,
        issuer: &mandate.issuer,
        tool_name: &tool_call.tool_name,
        operation_class: tool_call.operation_class.as_str(),
        source_run_id: tool_call.source_run_id.as_deref(),
    })?;

    Ok(receipt)
}
