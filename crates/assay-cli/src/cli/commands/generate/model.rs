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

pub fn serialize(policy: &Policy, format: &str) -> Result<String> {
    Ok(match format {
        "json" => serde_json::to_string_pretty(policy)?,
        _ => serde_yaml::to_string(policy)?,
    })
}
