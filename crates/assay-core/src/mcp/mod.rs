pub mod audit;
pub mod decision;
pub(crate) mod era;
#[cfg(test)]
mod era_parity_tests;
pub mod g3_auth_context;
pub mod modern_wire;
#[cfg(test)]
mod modern_wire_tests;

pub use g3_auth_context::AuthContextProjection;
pub mod identity;
pub mod ingest;
pub mod jcs;
pub(crate) mod json_depth;
pub mod jsonrpc;
pub mod unique_json;
pub use jsonrpc::is_tools_call_method;
pub use unique_json::{is_method_bearing_frame, parse_unique_json, UniqueJsonError};
pub mod lifecycle;
pub mod mapper_v2;
pub mod obligations;
pub mod parser;
pub mod policy;
pub mod proxy;
pub mod runtime_features;
pub mod signing;
pub mod tool_call_handler;
pub mod tool_decision_truth;
pub mod tool_definition;
pub mod tool_match;
pub mod tool_taxonomy;
pub mod trust_policy;
pub mod types;

pub use mapper_v2::*;
pub use parser::*;
pub use signing::{sign_tool, verify_tool, ToolSignature, VerifyError, VerifyResult};
pub use tool_match::{MatchBasis, MatchResult, ToolContext, ToolRuleSelector};
pub use tool_taxonomy::ToolTaxonomy;
pub use trust_policy::TrustPolicy;
pub use types::*;

#[cfg(test)]
pub mod tests;
