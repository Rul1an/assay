use crate::json_strict::errors::StrictJsonError;
use std::str::CharIndices;

pub(crate) fn parse_json_string_impl<'a>(
    chars: &mut std::iter::Peekable<CharIndices<'a>>,
) -> Result<String, StrictJsonError> {
    let _max_string_length = super::limits::MAX_STRING_LENGTH;
    crate::json_strict::scan::parse_json_string(chars)
}
