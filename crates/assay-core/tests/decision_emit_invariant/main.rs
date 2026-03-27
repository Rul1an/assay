//! Integration tests for decision emission invariant I1.
//!
//! These tests verify that every tool call attempt results in exactly
//! one decision event being emitted, regardless of outcome.

mod approval;
mod delegation;
mod emission;
mod fixtures;
mod g3_auth;
mod guard;
mod redaction;
mod restrict_scope;
