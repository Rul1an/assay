//! Delegation capability spoofing with provenance ambiguity.
//!
//! Tests whether schema-valid, protocol-plausible capability claims,
//! provenance signals, or identity metadata from a delegated actor can cause
//! downstream consumers to silently upgrade trust or weaken classification.
//!
//! 4 attack vectors + 3 benign controls. All deterministic, no LLM calls.

#[path = "delegation_spoofing_next/mod.rs"]
mod delegation_spoofing_next;

pub use delegation_spoofing_next::{
    control_d1_legitimate_upgrade, control_d2_legitimate_lossy, control_d3_adapter_migration,
    run_delegation_spoofing_matrix, vector1_capability_overclaim, vector2_provenance_ambiguity,
    vector3_identity_spoofing, vector4_selection_manipulation, DelegationOutcome, DelegationResult,
};
