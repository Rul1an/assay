use crate::declared_manifest::{
    construct_declared_manifest, is_canonical_sha256, parse_declared_manifest, BaselineTool,
    DeclaredManifest, DeclaredServer,
};
use crate::manifest_io::{read_bounded_bytes, write_json_create_new};
use crate::manifest_observed::{manifest_digest, CANONICALIZATION, NON_CLAIMS, SCHEMA};
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, HashSet};
use std::path::Path;

pub const CANDIDATE_SCHEMA: &str = "assay.mcp_manifest_candidate.v0";
const CANDIDATE_NON_CLAIMS: [&str; 4] = [
    "candidate is not an approval",
    "source_sha256 proves byte identity, not provenance",
    "does not prove the observed server is honest",
    "does not judge whether any tool is safe",
];

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedServer {
    id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedTool {
    name: String,
    tool_digest: String,
    privileged: bool,
    privilege_classification: String,
    action_class: Option<String>,
    field_digests: Option<BTreeMap<String, String>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedBody {
    manifest_digest: Option<String>,
    canonicalization: String,
    tool_count: usize,
    privileged_tool_count: usize,
    tools_list_observed: bool,
    tools_list_complete: String,
    tool_digests: Vec<ObservedTool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ObservedManifest {
    schema: String,
    status: String,
    server: ObservedServer,
    observed: ObservedBody,
    non_claims: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct ManifestCandidate {
    pub schema: String,
    pub status: String,
    pub approval: String,
    pub source_sha256: String,
    pub canonicalization: String,
    pub manifest_digest: String,
    pub server: DeclaredServer,
    pub tools: Vec<BaselineTool>,
    pub non_claims: Vec<String>,
}

pub fn export_candidate(from_observed: &Path, out: &Path) -> Result<()> {
    let source = read_bounded_bytes(from_observed)?;
    let candidate = candidate_from_source(&source)?;
    write_json_create_new(out, &candidate)
}

pub fn promote(candidate_path: &Path, source_path: &Path, out: &Path) -> Result<DeclaredManifest> {
    let candidate_bytes = read_bounded_bytes(candidate_path)?;
    let candidate = parse_candidate(&candidate_bytes)?;
    let source_bytes = read_bounded_bytes(source_path)?;
    let reconstructed = candidate_from_source(&source_bytes)?;
    if candidate != reconstructed {
        bail!("candidate does not match the exact observed source bytes");
    }

    let declared = declared_from_candidate(&candidate)?;
    let rendered = serde_json::to_string_pretty(&declared)?;
    parse_declared_manifest(&rendered)
        .context("promotion produced an invalid declared manifest")?;
    write_json_create_new(out, &declared)?;
    Ok(declared)
}

fn parse_candidate(bytes: &[u8]) -> Result<ManifestCandidate> {
    let text = std::str::from_utf8(bytes).context("candidate must be valid UTF-8")?;
    let value = assay_core::mcp::parse_unique_json(text)?;
    let candidate: ManifestCandidate = serde_json::from_value(value)?;
    validate_candidate(candidate)
}

fn candidate_from_source(bytes: &[u8]) -> Result<ManifestCandidate> {
    let observed = parse_promotable_observed(bytes)?;
    let mut tools: Vec<BaselineTool> = observed
        .observed
        .tool_digests
        .into_iter()
        .map(|tool| BaselineTool {
            name: tool.name,
            tool_digest: tool.tool_digest,
            privileged: Some(tool.privileged),
            privilege_classification: Some(tool.privilege_classification),
            action_class: tool.action_class,
            field_digests: tool.field_digests,
        })
        .collect();
    tools.sort_by(|left, right| left.name.cmp(&right.name));

    validate_candidate(ManifestCandidate {
        schema: CANDIDATE_SCHEMA.to_string(),
        status: "candidate".to_string(),
        approval: "not_approved".to_string(),
        source_sha256: format!("sha256:{}", hex::encode(Sha256::digest(bytes))),
        canonicalization: observed.observed.canonicalization,
        manifest_digest: observed.observed.manifest_digest.expect("validated above"),
        server: DeclaredServer {
            id: observed.server.id,
        },
        tools,
        non_claims: CANDIDATE_NON_CLAIMS
            .iter()
            .map(|value| value.to_string())
            .collect(),
    })
}

fn parse_promotable_observed(bytes: &[u8]) -> Result<ObservedManifest> {
    let text = std::str::from_utf8(bytes).context("observed manifest must be valid UTF-8")?;
    let value = assay_core::mcp::parse_unique_json(text)?;
    let manifest: ObservedManifest = serde_json::from_value(value)?;

    if manifest.schema != SCHEMA || manifest.status != "observed" {
        bail!("source must be an observed {SCHEMA} artifact");
    }
    if !manifest.observed.tools_list_observed
        || manifest.observed.tools_list_complete != "complete"
        || manifest.observed.manifest_digest.is_none()
    {
        bail!("observed source must carry one complete, unambiguous tools/list");
    }
    if manifest.observed.canonicalization != CANONICALIZATION {
        bail!("observed source uses an unsupported canonicalization");
    }
    if manifest.observed.tool_digests.is_empty()
        || manifest.observed.tool_count != manifest.observed.tool_digests.len()
    {
        bail!("observed source must carry a non-empty, count-consistent tool list");
    }
    let privileged_count = manifest
        .observed
        .tool_digests
        .iter()
        .filter(|tool| tool.privileged)
        .count();
    if privileged_count != manifest.observed.privileged_tool_count {
        bail!("observed source privileged_tool_count does not match its tools");
    }

    let mut names = HashSet::new();
    for tool in &manifest.observed.tool_digests {
        if tool.name.trim().is_empty() || !names.insert(tool.name.as_str()) {
            bail!("observed source tool names must be non-empty and unique");
        }
    }
    let pairs: Vec<(String, String)> = manifest
        .observed
        .tool_digests
        .iter()
        .map(|tool| (tool.name.clone(), tool.tool_digest.clone()))
        .collect();
    if manifest_digest(&pairs) != manifest.observed.manifest_digest.as_deref().unwrap() {
        bail!(
            "observed source manifest_digest cannot be reproduced from its serialized tool identities"
        );
    }

    if manifest.non_claims
        != NON_CLAIMS
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    {
        bail!("observed source non_claims do not match the observed-v0 contract");
    }
    Ok(manifest)
}

fn validate_candidate(candidate: ManifestCandidate) -> Result<ManifestCandidate> {
    if candidate.schema != CANDIDATE_SCHEMA
        || candidate.status != "candidate"
        || candidate.approval != "not_approved"
    {
        bail!("candidate status does not describe a non-approved candidate");
    }
    if candidate.non_claims
        != CANDIDATE_NON_CLAIMS
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>()
    {
        bail!("candidate non_claims do not match the closed candidate contract");
    }
    if !is_canonical_sha256(&candidate.source_sha256) {
        bail!("candidate source_sha256 is not canonical sha256");
    }
    declared_from_candidate(&candidate)?;
    Ok(candidate)
}

fn declared_from_candidate(candidate: &ManifestCandidate) -> Result<DeclaredManifest> {
    construct_declared_manifest(
        candidate.canonicalization.clone(),
        candidate.manifest_digest.clone(),
        candidate.tools.clone(),
        Some(candidate.server.clone()),
    )
}
