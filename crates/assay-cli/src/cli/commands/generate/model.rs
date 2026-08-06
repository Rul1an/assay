use crate::cli::args::common::PolicyOutputFormat;
use anyhow::Result;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Policy {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub _meta: Option<Meta>,
    pub files: Section,
    pub network: NetSection,
    pub processes: Section,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Meta {
    pub name: String,
    pub generated_at: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub profile_runs: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_stability: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_runs: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Section {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_review: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct NetSection {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allow_destinations: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub needs_review: Vec<Entry>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub deny_destinations: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum Entry {
    Simple(String),
    WithMeta {
        pattern: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        count: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        stability: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        runs_seen: Option<u32>,
        #[serde(skip_serializing_if = "Option::is_none")]
        risk: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reasons: Option<Vec<String>>,
    },
}

/// Render a policy in the requested format.
///
/// Takes the enum, not a `&str`. The `&str` version's `_` arm wrote YAML, so `--format jsom`
/// produced a YAML policy at exit 0 — into whatever path the user named, `.json` included. With
/// the enum there is no arm left for a value nobody chose.
pub fn serialize(policy: &Policy, format: PolicyOutputFormat) -> Result<String> {
    Ok(match format {
        PolicyOutputFormat::Json => serde_json::to_string_pretty(policy)?,
        PolicyOutputFormat::Yaml => serde_yaml::to_string(policy)?,
    })
}
