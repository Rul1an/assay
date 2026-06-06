//! Protocol Evidence Interpretation Attacks.
//!
//! Tests consumer-side trust downgrade under partial, ambiguous, or flattened
//! protocol evidence. When does protocol-valid but incompletely interpreted
//! metadata lead to an overly optimistic trust decision?
//!
//! 4 attack vectors + 3 benign controls. All deterministic, no LLM calls.

#[path = "consumer_downgrade_next/mod.rs"]
mod consumer_downgrade_next;

pub use consumer_downgrade_next::{
    control_e1_legitimate_legacy, control_e2_legitimate_compat, control_e3_legitimate_converged,
    run_consumer_downgrade_matrix, vector1_partial_trust_read, vector2_precedence_inversion,
    vector3_compat_flattening, vector4_projection_loss, ConsumerOutcome, ConsumerResult,
};
