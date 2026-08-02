mod serde;
mod types;
mod validation;

pub(crate) use serde::parse_expected_entry;
pub use types::*;
pub use validation::SEMANTIC_SIMILARITY_EPSILON;
pub(crate) use validation::{vacuous_expected_field, validate_test_case_for_execution};

#[cfg(test)]
mod tests;
