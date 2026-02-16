use super::super::{AuthorizeError, Authorizer, MandateData, ToolCallData};
use super::{policy, store};
use chrono::{DateTime, Duration, Utc};

pub(crate) fn authorize_at_impl(
    authorizer: &Authorizer,
    now: DateTime<Utc>,
    mandate: &MandateData,
    tool_call: &ToolCallData,
) -> Result<super::super::AuthzReceipt, AuthorizeError> {
    let skew = Duration::seconds(authorizer.config.clock_skew_seconds);

    policy::check_validity_window_impl(now, mandate, skew)?;
    store::check_revocation_impl(authorizer, now, mandate)?;
    policy::check_context_impl(&authorizer.config, mandate)?;
    policy::check_scope_impl(tool_call, mandate)?;
    policy::check_operation_class_impl(mandate, tool_call)?;
    policy::check_transaction_ref_impl(mandate, tool_call)?;
    store::upsert_mandate_metadata_impl(authorizer, mandate)?;
    let receipt = store::consume_mandate_impl(authorizer, mandate, tool_call)?;

    Ok(receipt)
}

pub(crate) fn authorize_and_consume_impl(
    authorizer: &Authorizer,
    mandate: &MandateData,
    tool_call: &ToolCallData,
) -> Result<super::super::AuthzReceipt, AuthorizeError> {
    authorize_at_impl(authorizer, Utc::now(), mandate, tool_call)
}
