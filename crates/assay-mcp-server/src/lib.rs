#![deny(unsafe_code)]

/// Test-only backstop shared with the binary's test build (see `main.rs`).
#[cfg(test)]
mod claims_backstop;

pub mod auth;
pub mod cache;
pub mod config;
pub mod declared_manifest;
pub mod enforcement_sarif;
pub mod manifest_io;
pub mod manifest_observed;
pub mod manifest_promotion;
pub mod modern_adapter;
pub mod security;
pub mod server;
pub mod side_effect;
pub mod token_passthrough;
pub mod tool_decision;
pub mod tools;
