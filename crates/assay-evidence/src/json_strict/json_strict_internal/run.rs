use crate::json_strict::errors::StrictJsonError;
use serde::de::DeserializeOwned;

pub(crate) fn from_str_strict_impl<T: DeserializeOwned>(s: &str) -> Result<T, StrictJsonError> {
    validate_json_strict_impl(s)?;
    Ok(serde_json::from_str(s)?)
}

pub(crate) fn validate_json_strict_impl(s: &str) -> Result<(), StrictJsonError> {
    let mut validator = super::validate::JsonValidator::new(s);
    validator.validate()
}

pub(crate) fn validate_json_strict_with_depth_impl(
    s: &str,
    max_depth: usize,
) -> Result<(), StrictJsonError> {
    let mut validator = super::validate::JsonValidator::with_max_depth(s, max_depth);
    validator.validate()
}
