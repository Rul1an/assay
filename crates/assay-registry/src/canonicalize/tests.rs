//! Canonicalize behavior freeze tests.
//!
//! Kept in separate files so mod.rs grep-gates (forbidden-knowledge) target
//! implementation code only, not test code.

mod duplicates;
mod golden;
mod rejections;
mod values;

mod support {
    pub(super) use super::super::*;
    pub(super) use serde_json::Value as JsonValue;
}
