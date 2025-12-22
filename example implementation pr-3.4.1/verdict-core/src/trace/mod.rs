//! Trace management for deterministic replay.
//!
//! This module provides:
//! - Trace loading and validation
//! - Coverage verification against config
//! - Closest match hints for missing entries

mod verify;

pub use verify::{
    format_verify_result, EmbeddingMeta, JudgeMeta, MissingTest, StrictReplayStatus,
    TestCase, TraceEntry, TraceMeta, TraceVerifier, VerifyResult,
};
