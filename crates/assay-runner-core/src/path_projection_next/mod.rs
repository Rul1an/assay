mod project;

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

pub const PATH_PROJECTION_SCHEMA: &str = "assay.runner.path_projection.v0";
const TAXONOMY_SCHEMA: &str = "assay.runner.runtime_noise_taxonomy.v0";

const STATUS_APPLIED: &str = "applied";
const CLAIM_LEVEL_PROJECTED_EQUIVALENT: &str = "projected_equivalent";
const CLAIM_LEVEL_INCONCLUSIVE: &str = "inconclusive";
const CONFIDENCE_DECLARED: &str = "declared";

const PATH_CLASS_WORKLOAD_FIXTURE: &str = "workload_fixture";
const PATH_CLASS_UNKNOWN: &str = "unknown";

const RELATION_WORKLOAD_CONTRACT: &str = "declared_workload_contract";
const RELATION_INSIDE_RUN_WORKDIR: &str = "inside_run_workdir";

const RULE_WORKLOAD_INPUT: &str = "workload_contract_input_path";
const RULE_WORKLOAD_OUTPUT: &str = "workload_contract_output_path";
const RULE_WORKLOAD_SCRATCH: &str = "workload_contract_scratch_path";
const RULE_DECLARED_WORKDIR_PREFIX: &str = "declared_run_workdir_prefix";

const NON_CLAIMS: &[&str] = &[
    "projection_no_raw_evidence_rewrite",
    "projection_no_semantic_workload_equivalence",
    "projection_no_policy_acceptability_verdict",
    "projection_unknowns_preserved",
    "projection_no_heuristic_noise_taxonomy",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredPathProjectionRules {
    exact_rules: Vec<DeclaredPathRule>,
    workdir_prefixes: Vec<String>,
    sample_limit: usize,
}

impl Default for DeclaredPathProjectionRules {
    fn default() -> Self {
        Self {
            exact_rules: Vec::new(),
            workdir_prefixes: Vec::new(),
            sample_limit: 5,
        }
    }
}

impl DeclaredPathProjectionRules {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with_sample_limit(mut self, sample_limit: usize) -> Self {
        self.sample_limit = sample_limit;
        self
    }

    pub fn add_rule(&mut self, rule: DeclaredPathRule) -> &mut Self {
        self.exact_rules.push(rule);
        self
    }

    pub fn add_workload_input_path(&mut self, raw_path: impl Into<String>) -> &mut Self {
        self.add_rule(DeclaredPathRule::workload_input(raw_path))
    }

    pub fn add_workload_output_path(&mut self, raw_path: impl Into<String>) -> &mut Self {
        self.add_rule(DeclaredPathRule::workload_output(raw_path))
    }

    pub fn add_workload_scratch_path(&mut self, raw_path: impl Into<String>) -> &mut Self {
        self.add_rule(DeclaredPathRule::workload_scratch(raw_path))
    }

    pub fn add_workdir_prefix(&mut self, prefix: impl Into<String>) -> &mut Self {
        let prefix = normalize_prefix(prefix.into());
        if !prefix.is_empty() && !self.workdir_prefixes.contains(&prefix) {
            self.workdir_prefixes.push(prefix);
            self.workdir_prefixes.sort();
        }
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclaredPathRule {
    raw_path: String,
    projected_path: String,
    path_class: String,
    relation: String,
    rule: String,
}

impl DeclaredPathRule {
    #[must_use]
    pub fn new(
        raw_path: impl Into<String>,
        projected_path: impl Into<String>,
        path_class: impl Into<String>,
        relation: impl Into<String>,
        rule: impl Into<String>,
    ) -> Self {
        Self {
            raw_path: raw_path.into(),
            projected_path: projected_path.into(),
            path_class: path_class.into(),
            relation: relation.into(),
            rule: rule.into(),
        }
    }

    #[must_use]
    pub fn workload_input(raw_path: impl Into<String>) -> Self {
        Self::new(
            raw_path,
            "workdir/input",
            PATH_CLASS_WORKLOAD_FIXTURE,
            RELATION_WORKLOAD_CONTRACT,
            RULE_WORKLOAD_INPUT,
        )
    }

    #[must_use]
    pub fn workload_output(raw_path: impl Into<String>) -> Self {
        Self::new(
            raw_path,
            "workdir/output",
            PATH_CLASS_WORKLOAD_FIXTURE,
            RELATION_WORKLOAD_CONTRACT,
            RULE_WORKLOAD_OUTPUT,
        )
    }

    #[must_use]
    pub fn workload_scratch(raw_path: impl Into<String>) -> Self {
        Self::new(
            raw_path,
            "workdir/scratch",
            PATH_CLASS_WORKLOAD_FIXTURE,
            RELATION_WORKLOAD_CONTRACT,
            RULE_WORKLOAD_SCRATCH,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathProjection {
    pub schema: String,
    pub status: String,
    pub taxonomy_schema: String,
    pub claim_level: String,
    pub mappings: Vec<PathProjectionMapping>,
    pub rules: Vec<String>,
    pub unmatched_summary: UnmatchedPathSummary,
    pub non_claims: Vec<String>,
}

impl PathProjection {
    #[must_use]
    pub fn projected_paths(&self) -> BTreeSet<String> {
        self.mappings
            .iter()
            .map(|mapping| mapping.projected_path.clone())
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PathProjectionMapping {
    pub raw_path: String,
    pub projected_path: String,
    pub path_class: String,
    pub relation: String,
    pub rule: String,
    pub confidence: String,
    pub claim_level: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UnmatchedPathSummary {
    pub path_class: String,
    pub count: usize,
    pub samples: Vec<String>,
    pub sample_limit: usize,
}

fn normalize_prefix(mut prefix: String) -> String {
    while prefix.len() > 1 && prefix.ends_with('/') {
        prefix.pop();
    }
    prefix
}

pub use project::project_filesystem_paths;

#[cfg(test)]
mod tests;
