mod controls;
mod fixtures;
mod matrix;
mod types;
mod vectors;

pub use controls::{
    control_e1_legitimate_legacy, control_e2_legitimate_compat, control_e3_legitimate_converged,
};
pub use matrix::run_consumer_downgrade_matrix;
pub use types::{ConsumerOutcome, ConsumerResult};
pub use vectors::{
    vector1_partial_trust_read, vector2_precedence_inversion, vector3_compat_flattening,
    vector4_projection_loss,
};

#[cfg(test)]
mod tests;
