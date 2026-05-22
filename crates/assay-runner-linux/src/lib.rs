//! Linux-only platform adapter for the Assay-Runner candidate.
//!
//! This crate is the Phase 2D Slice 3 result of the Assay-Runner extraction
//! roadmap (see `docs/reference/runner/extraction-roadmap.md`). It hosts
//! Linux-specific runner platform primitives that previously lived inside
//! `crates/assay-cli/` and therefore prevented the runner candidate from
//! being moved without dragging `assay-cli` along.
//!
//! Current scope is intentionally narrow: only cgroup v2 placement
//! (`CgroupManager`, `SessionCgroup`). No eBPF or monitor adapter lives
//! here yet — that boundary work is Slice 4 per the roadmap, and is not
//! opened by this crate's existence. Likewise, this crate does not
//! provide a platform-abstraction trait; macOS and Windows support are
//! out of scope until separate platform spikes open under
//! `platform-and-extraction-readiness.md`.
//!
//! The crate is `publish = false` until Slice 7 (repository extraction).

mod cgroup;

pub use cgroup::{CgroupManager, SessionCgroup};
