//! Environment variable filtering for sandbox security.
//!
//! By default, the sandbox scrubs sensitive environment variables to prevent
//! credential leakage to untrusted MCP servers and agents. This module
//! implements the filtering logic described in ADR-001 and Phase 6 Hardening.
//!
//! # Security Model
//!
//! - **Scrub (Default)**: Remove known sensitive patterns (API keys, tokens, secrets)
//! - **Strict**: Only allow safe base variables (PATH, HOME, etc) + explicit allows
//! - **Passthrough**: Danger mode - pass all variables (not recommended)
//!
//! Additionally, "Execution Influence" variables (like LD_PRELOAD) can be stripped
//! independently or as part of Strict mode.

mod engine;
mod matcher;
mod patterns;

#[allow(unused_imports)]
pub use engine::{EnvFilter, EnvFilterResult, EnvMode};
pub use matcher::matches_any_pattern;
#[allow(unused_imports)]
pub use patterns::{EXEC_INFLUENCE_PATTERNS, SAFE_BASE_PATTERNS, SECRET_SCRUB_PATTERNS};

#[cfg(test)]
mod tests;
