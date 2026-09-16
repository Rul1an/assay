//! Deterministic replay verifier for `gateway-path.v0` evidence bundles.

pub mod cap1;
pub mod replay;
pub mod schema;

pub use cap1::{
    verify_cap1_document, verify_cap1_rules, verify_cap1_rules_with, Cap1AdmissionLimits,
    Cap1Document, Cap1NormativeRule, Cap1Refusal, Cap1Stage, Cap1SyntaxFault, CAP1_SCHEMA_JSON,
    CAP1_SCHEMA_SHA256, CAP1_SCHEMA_SOURCE,
};
pub use replay::{verify_bundle, verify_json_str, verify_json_value, VerifyError};
pub use schema::{
    Ceiling, EvidenceBundle, NonClaim, Reason, ReplayResult, SourceClass, Status, PROFILE,
};
