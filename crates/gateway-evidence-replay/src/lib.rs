//! Deterministic replay verifier for `gateway-path.v0` evidence bundles.

pub mod replay;
pub mod schema;

pub use replay::{verify_bundle, verify_json_str, verify_json_value, VerifyError};
pub use schema::{
    Ceiling, EvidenceBundle, NonClaim, Reason, ReplayResult, SourceClass, Status, PROFILE,
};
