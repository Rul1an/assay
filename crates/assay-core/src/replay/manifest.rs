//! Replay bundle manifest schema (E9).
//!
//! Normative manifest for the Replay Bundle: schema_version 1, digests,
//! replay_coverage (complete_tests / incomplete_tests / reason), toolchain,
//! seeds, scrub policy, and file manifest. See E9-REPLAY-BUNDLE-PLAN and
//! SPEC-Replay-Bundle-v1.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Manifest schema version for replay bundle v1.
pub const REPLAY_MANIFEST_SCHEMA_VERSION: u32 = 1;

/// Replay bundle manifest (manifest.json at bundle root).
///
/// Single source of truth for bundle contents. All paths use POSIX forward
/// slashes and are relative to the bundle root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayManifest {
    /// Schema version; MUST be 1 for this spec.
    pub schema_version: u32,

    /// Assay CLI version that produced the run (e.g. "2.15.0").
    pub assay_version: String,

    /// ISO 8601 UTC when the bundle was created.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,

    /// Path that was used as source for this bundle (audit). E.g. ".assay/run_abc123" or path to run.json.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_run_path: Option<String>,

    /// How source run was selected (e.g. "run-id", "mtime-latest", "explicit-from").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selection_method: Option<String>,

    /// Git SHA of the repo at bundle creation; dirty flag if uncommitted changes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_sha: Option<String>,

    #[serde(skip_serializing_if = "Option::is_none")]
    pub git_dirty: Option<bool>,

    /// CI workflow run ID if available.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workflow_run_id: Option<String>,

    /// Digest of config file (e.g. "sha256:...").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_digest: Option<String>,

    /// Digest of policy/pack.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub policy_digest: Option<String>,

    /// Digest of baseline if applicable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub baseline_digest: Option<String>,

    /// Digest of trace input (primary trace).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_digest: Option<String>,

    /// Relative path inside bundle to trace file(s), e.g. "files/trace.jsonl".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace_path: Option<String>,

    /// Output paths (relative to bundle root). E.g. outputs/run.json, outputs/summary.json.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub outputs: Option<ReplayOutputs>,

    /// Toolchain and runner metadata (E9.2). Required; use "unknown" where unavailable.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub toolchain: Option<ToolchainMeta>,

    /// Seeds for deterministic replay (E7.2). Present when run had seeds.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seeds: Option<ReplaySeeds>,

    /// Which tests are fully replayable vs incomplete (E9.1). Normative single field.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replay_coverage: Option<ReplayCoverage>,

    /// Scrub policy: default deny (include_prompts false, scrub_cassettes allowlist). E9.4.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrub_policy: Option<ScrubPolicy>,

    /// File manifest: relative path -> hash and size. POSIX slashes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub files: Option<BTreeMap<String, FileManifestEntry>>,

    /// Legacy/env free-form (e.g. runner label). Prefer toolchain for structured data.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<BTreeMap<String, serde_json::Value>>,
}

/// Paths to outputs inside the bundle (relative to bundle root).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayOutputs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub junit: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sarif: Option<String>,
}

/// Toolchain and runner metadata (E9.2). Fields required but "unknown" allowed.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainMeta {
    /// rustc -Vv output or version string.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rustc: Option<String>,

    /// cargo -V.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cargo: Option<String>,

    /// SHA256 of Cargo.lock (or "unknown").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cargo_lock_hash: Option<String>,

    /// Runner context: os, arch, image, CI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runner: Option<RunnerMeta>,
}

/// Runner identity for "works on my machine" debugging.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RunnerMeta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
    /// Container image digest or tag+digest if in CI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_image_digest: Option<String>,
    /// True when running in CI.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ci: Option<bool>,
}

/// Seeds used in the original run (E7.2). Serialized as string or null.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplaySeeds {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seed_version: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order_seed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub judge_seed: Option<String>,
}

/// Normative replay coverage: which tests are fully replayable vs incomplete (E9.1).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayCoverage {
    /// Test IDs that have all inputs (trace + config + cassette if needed).
    pub complete_tests: Vec<String>,

    /// Test IDs that are missing a dependency (e.g. judge not cached).
    pub incomplete_tests: Vec<String>,

    /// For each test_id in incomplete_tests, short reason (e.g. "judge response not cached").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<BTreeMap<String, String>>,
}

/// Scrub policy in manifest: default deny (E9.4). Implementation in E9b.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScrubPolicy {
    /// Include prompts in bundle (default false).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub include_prompts: Option<bool>,

    /// Scrub cassettes with allowlist (default true = deny-by-default).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scrub_cassettes: Option<bool>,

    /// Policy name, e.g. "default".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Default for ScrubPolicy {
    fn default() -> Self {
        Self {
            include_prompts: Some(false),
            scrub_cassettes: Some(true),
            name: Some("default".into()),
        }
    }
}

/// Single file entry in the bundle file manifest. POSIX path; sha256 and size required.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FileManifestEntry {
    /// SHA256 hex or "sha256:..." prefix.
    pub sha256: String,
    /// Size in bytes.
    pub size: u64,
    /// Optional mode (e.g. 0o644).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<u32>,
    /// Optional content-type hint.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_type: Option<String>,
}

impl ReplayManifest {
    /// Build a minimal valid v1 manifest (schema_version + assay_version).
    pub fn minimal(assay_version: String) -> Self {
        Self {
            schema_version: REPLAY_MANIFEST_SCHEMA_VERSION,
            assay_version,
            created_at: None,
            source_run_path: None,
            selection_method: None,
            git_sha: None,
            git_dirty: None,
            workflow_run_id: None,
            config_digest: None,
            policy_digest: None,
            baseline_digest: None,
            trace_digest: None,
            trace_path: None,
            outputs: None,
            toolchain: None,
            seeds: None,
            replay_coverage: None,
            scrub_policy: Some(ScrubPolicy::default()),
            files: None,
            env: None,
        }
    }
}
