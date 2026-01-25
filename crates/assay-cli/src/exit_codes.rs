//! Unified exit codes for Assay Sandbox.
//! These codes are part of the public contract and ensure consistent behavior across backends.

pub const SUCCESS: i32 = 0;
pub const COMMAND_FAILED: i32 = 1; // Child process returned non-zero
pub const INTERNAL_ERROR: i32 = 2; // Assay setup/attach failed or config error
pub const VIOLATION_AUDIT: i32 = 3; // Policy violation (Audit mode)
pub const WOULD_BLOCK: i32 = 4; // Would block (Dry-run mode)
