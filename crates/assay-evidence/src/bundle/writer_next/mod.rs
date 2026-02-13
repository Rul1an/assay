//! Step-3 scaffold for writer split.
//!
//! `src/bundle/writer.rs` remains the active public implementation in this
//! commit. `writer_next/*` is intentionally not wired yet to keep behavior
//! unchanged while preparing mechanical moves.
//!
//! Facade boundary intent for the target layout:
//! - `write.rs`: write orchestration
//! - `verify.rs`: verify orchestration
//! - `manifest.rs`: manifest structs/serialization helpers
//! - `events.rs`: NDJSON/event helpers
//! - `tar_write.rs`: deterministic tar.gz write path only
//! - `tar_read.rs`: tar/gzip read/iterate helpers only
//! - `limits.rs`: single source of truth for verification limits
//! - `errors.rs`: typed error helpers
//! - `tests.rs`: relocation placeholder

pub(crate) mod errors;
pub(crate) mod events;
pub(crate) mod limits;
pub(crate) mod manifest;
pub(crate) mod tar_read;
pub(crate) mod tar_write;
pub(crate) mod tests;
pub(crate) mod verify;
pub(crate) mod write;
