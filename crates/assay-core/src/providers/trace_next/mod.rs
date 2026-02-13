//! Wave3 Step2 scaffold for trace provider split.
//!
//! Contract:
//! - Existing public API remains in `trace.rs` facade for Step2.
//! - This module is scaffold-only in Commit A (no behavior wiring).

pub(crate) mod errors;
pub(crate) mod io;
pub(crate) mod normalize;
pub(crate) mod parse;
pub(crate) mod tests;
pub(crate) mod v2;
