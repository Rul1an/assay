//! Wave3 Step2 scaffold for monitor split.
//!
//! Contract:
//! - Existing public API remains in `monitor.rs` facade for Step2.
//! - This module is scaffold-only in Commit A (no behavior wiring).

pub(crate) mod errors;
pub(crate) mod events;
pub(crate) mod normalize;
pub(crate) mod output;
pub(crate) mod rules;
pub(crate) mod syscall_linux;
pub(crate) mod tests;
