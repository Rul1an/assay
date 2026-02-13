#![cfg(target_os = "linux")]

//! Linux-only syscall boundary for monitor flow.
//!
//! Boundary:
//! - all unsafe syscall operations live here
//! - linux cfg gating is centralized here
