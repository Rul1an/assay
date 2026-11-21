//! The proxy loads its bounded operator policy before starting the upstream.
#![forbid(dead_code)]

use super::manifest::DeclaredManifest;
use anyhow::{Context, Result};
use assay_common::enforce_document;
pub use assay_common::enforce_document::EnforcePolicy;
use assay_common::ignored_keys::{Error, MAX_BYTES};
use assay_common::limits::{LimitKind, LimitReader};
use std::io::Read;
use std::path::{Path, PathBuf};

/// The enforce-mode inputs to the proxy, grouped so `run` stays within a sane arity. All fields are
/// absent in observe mode; `policy` and `baseline` are always present in enforce mode (loaded at
/// startup), `decision_out` is the optional P61e-d evidence path, `establish_out` is the optional
/// `assay.manifest_establish.v0` carrier path (Increment 2c), and `establish_budget` is the one total
/// deadline for a pre-call establish run.
#[derive(Default)]
pub struct EnforceInputs {
    pub policy: Option<EnforcePolicy>,
    pub baseline: Option<DeclaredManifest>,
    pub decision_out: Option<PathBuf>,
    /// Optional NDJSON path for `assay.denied_call_observation.v1`: the caller-visible proxy-deny
    /// surface, bound to the call's tool and target digest when classification provides one. This is
    /// an observation carrier, not the policy verdict.
    pub denied_call_observation_out: Option<PathBuf>,
    pub establish_out: Option<PathBuf>,
    /// Optional NDJSON path for the per-call `assay.tool_annotation_conformance.v0` carrier
    /// (Increment 5b): the server's declared annotation hints vs Assay's observed call
    /// classification. Orthogonal to the verdict; on an allowed call a write failure fails closed,
    /// the same evidence rule as the other carriers.
    pub tool_conformance_out: Option<PathBuf>,
    pub establish_budget: std::time::Duration,
}

/// Load + validate the enforce policy. Any failure here is a STARTUP failure (the caller surfaces it
/// as a non-zero exit), never a runtime deny: an enforcing proxy without a valid policy is a
/// misconfigured service and must not start.
///
/// A key the proxy does not act on refuses the policy, named by its path. serde's defaults used to
/// drop it: an allowance target `{owner, repo, read_only: true}` allowed every deploy key on that
/// repository, and a misspelt `allowances` loaded as no allowance at all.
pub fn load(path: &Path) -> Result<EnforcePolicy> {
    let mut text = String::new();
    std::fs::File::open(path)
        .and_then(|file| {
            LimitReader::new(file, MAX_BYTES, LimitKind::SourceBytes).read_to_string(&mut text)
        })
        .with_context(|| format!("reading --enforce-policy {}", path.display()))?;
    use enforce_document::Error::{Document, EmptyCallerId};
    enforce_document::read(&text).map_err(|error| match error {
        EmptyCallerId => anyhow::anyhow!("--enforce-policy: caller.id must be a non-empty string"),
        Document(Error::Yaml(cause)) => anyhow::Error::new(cause).context("parsing --enforce-policy YAML"),
        Document(Error::TooLarge(cause)) => anyhow::Error::new(cause).context("parsing --enforce-policy YAML"),
        Document(Error::UnknownKeys(unread)) => {
            let keys: Vec<String> = unread.iter().map(|key| format!("{key:?}")).collect();
            anyhow::anyhow!("--enforce-policy: {} key(s) the enforcing proxy does not act on: {}. Correct or remove them.", keys.len(), keys.join(", "))
        }
        _ => anyhow::anyhow!("parsing --enforce-policy YAML: unsupported reader error"),
    })
}
