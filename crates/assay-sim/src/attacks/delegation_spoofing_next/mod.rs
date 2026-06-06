mod controls;
mod fixtures;
mod matrix;
mod types;
mod vectors;

pub use controls::{
    control_d1_legitimate_upgrade, control_d2_legitimate_lossy, control_d3_adapter_migration,
};
pub use matrix::run_delegation_spoofing_matrix;
pub use types::{DelegationOutcome, DelegationResult};
pub use vectors::{
    vector1_capability_overclaim, vector2_provenance_ambiguity, vector3_identity_spoofing,
    vector4_selection_manipulation,
};

#[cfg(test)]
mod tests;
