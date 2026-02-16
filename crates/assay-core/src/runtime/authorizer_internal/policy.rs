use super::super::{
    AuthorizeError, AuthzConfig, MandateData, OperationClass, PolicyError, ToolCallData,
};
use chrono::{DateTime, Duration, Utc};

pub(super) fn check_validity_window_impl(
    now: DateTime<Utc>,
    mandate: &MandateData,
    skew: Duration,
) -> Result<(), PolicyError> {
    if let Some(not_before) = mandate.not_before {
        if now < not_before - skew {
            return Err(PolicyError::NotYetValid { not_before, now });
        }
    }
    if let Some(expires_at) = mandate.expires_at {
        if now >= expires_at + skew {
            return Err(PolicyError::Expired { expires_at, now });
        }
    }
    Ok(())
}

pub(super) fn check_context_impl(
    config: &AuthzConfig,
    mandate: &MandateData,
) -> Result<(), PolicyError> {
    if !config.expected_audience.is_empty() && mandate.audience != config.expected_audience {
        return Err(PolicyError::AudienceMismatch {
            expected: config.expected_audience.clone(),
            actual: mandate.audience.clone(),
        });
    }

    if !config.trusted_issuers.is_empty() && !config.trusted_issuers.contains(&mandate.issuer) {
        return Err(PolicyError::IssuerNotTrusted {
            issuer: mandate.issuer.clone(),
        });
    }

    Ok(())
}

pub(super) fn check_scope_impl(
    tool_call: &ToolCallData,
    mandate: &MandateData,
) -> Result<(), PolicyError> {
    if !tool_matches_scope_impl(&tool_call.tool_name, &mandate.tool_patterns) {
        return Err(PolicyError::ToolNotInScope {
            tool: tool_call.tool_name.clone(),
        });
    }
    Ok(())
}

pub(super) fn check_operation_class_impl(
    mandate: &MandateData,
    tool_call: &ToolCallData,
) -> Result<(), PolicyError> {
    let max_allowed = mandate.mandate_kind.max_operation_class();
    if tool_call.operation_class > max_allowed {
        return Err(PolicyError::KindMismatch {
            kind: mandate.mandate_kind.as_str().to_string(),
            op_class: tool_call.operation_class.as_str().to_string(),
        });
    }
    Ok(())
}

pub(super) fn check_transaction_ref_impl(
    mandate: &MandateData,
    tool_call: &ToolCallData,
) -> Result<(), AuthorizeError> {
    if tool_call.operation_class == OperationClass::Commit {
        if let Some(expected_ref) = &mandate.transaction_ref {
            let tx_obj = tool_call
                .transaction_object
                .as_ref()
                .ok_or(PolicyError::MissingTransactionObject)?;

            let actual_ref = compute_transaction_ref_impl(tx_obj)
                .map_err(|e| AuthorizeError::TransactionRef(e.to_string()))?;

            if actual_ref != *expected_ref {
                return Err(PolicyError::TransactionRefMismatch {
                    expected: expected_ref.clone(),
                    actual: actual_ref,
                }
                .into());
            }
        }
    }

    Ok(())
}

pub(crate) fn tool_matches_scope_impl(tool_name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if glob_matches_impl(pattern, tool_name) {
            return true;
        }
    }
    false
}

pub(crate) fn glob_matches_impl(pattern: &str, input: &str) -> bool {
    let mut pattern_chars = pattern.chars().peekable();
    let mut input_chars = input.chars().peekable();

    while let Some(p) = pattern_chars.next() {
        match p {
            '*' => {
                if pattern_chars.peek() == Some(&'*') {
                    pattern_chars.next();
                    let remaining: String = pattern_chars.collect();
                    if remaining.is_empty() {
                        return true;
                    }
                    let remaining_input: String = input_chars.collect();
                    for i in 0..=remaining_input.len() {
                        if glob_matches_impl(&remaining, &remaining_input[i..]) {
                            return true;
                        }
                    }
                    return false;
                } else {
                    let remaining: String = pattern_chars.collect();
                    if remaining.is_empty() {
                        return input_chars.all(|c| c != '.');
                    }
                    let mut remaining_input: String = input_chars.collect();
                    loop {
                        if glob_matches_impl(&remaining, &remaining_input) {
                            return true;
                        }
                        if remaining_input.is_empty() || remaining_input.starts_with('.') {
                            return false;
                        }
                        remaining_input = remaining_input[1..].to_string();
                    }
                }
            }
            '\\' => {
                if let Some(escaped) = pattern_chars.next() {
                    if input_chars.next() != Some(escaped) {
                        return false;
                    }
                } else {
                    return false;
                }
            }
            c => {
                if input_chars.next() != Some(c) {
                    return false;
                }
            }
        }
    }

    input_chars.next().is_none()
}

pub(crate) fn compute_transaction_ref_impl(
    tx_object: &serde_json::Value,
) -> Result<String, String> {
    use sha2::{Digest, Sha256};

    let canonical = serde_jcs::to_vec(tx_object).map_err(|e| e.to_string())?;
    let hash = Sha256::digest(&canonical);
    Ok(format!("sha256:{}", hex::encode(hash)))
}
