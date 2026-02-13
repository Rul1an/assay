//! Test module boundary for Step-2 split.
//!
//! Commit A keeps tests in `src/verify.rs` active.
//! This module is reserved for Step-2 relocation in Commit B/C.

#![cfg(test)]

mod contract {
    // Reserved for behavior-freeze contract tests moved from verify.rs.
}

mod vectors {
    // Reserved for DSSE/digest vector tests moved from verify.rs.
}
