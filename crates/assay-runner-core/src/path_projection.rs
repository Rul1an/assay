use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

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

#[must_use]
pub fn project_filesystem_paths(
    raw_paths: &BTreeSet<String>,
    rules: &DeclaredPathProjectionRules,
) -> PathProjection {
    let exact_rules = exact_rule_map(rules);
    let mut mappings = Vec::new();
    let mut unmatched = Vec::new();

    for raw_path in raw_paths {
        if let Some(mapping) = map_exact(raw_path, raw_path, None, &exact_rules) {
            mappings.push(mapping);
            continue;
        }

        let (operation, path_suffix) = split_operation_path(raw_path);
        if let Some(mapping) = map_exact(raw_path, path_suffix, operation, &exact_rules) {
            mappings.push(mapping);
            continue;
        }

        if let Some(mapping) = map_workdir_prefix(raw_path, path_suffix, operation, rules) {
            mappings.push(mapping);
            continue;
        }

        unmatched.push(raw_path.clone());
    }

    mappings.sort_by(|left, right| {
        left.raw_path
            .cmp(&right.raw_path)
            .then_with(|| left.projected_path.cmp(&right.projected_path))
            .then_with(|| left.rule.cmp(&right.rule))
    });

    let rules_applied: Vec<String> = mappings
        .iter()
        .map(|mapping| mapping.rule.clone())
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();

    let claim_level = if mappings.is_empty() {
        CLAIM_LEVEL_INCONCLUSIVE
    } else {
        CLAIM_LEVEL_PROJECTED_EQUIVALENT
    };

    PathProjection {
        schema: PATH_PROJECTION_SCHEMA.to_string(),
        status: STATUS_APPLIED.to_string(),
        taxonomy_schema: TAXONOMY_SCHEMA.to_string(),
        claim_level: claim_level.to_string(),
        mappings,
        rules: rules_applied,
        unmatched_summary: UnmatchedPathSummary {
            path_class: PATH_CLASS_UNKNOWN.to_string(),
            count: unmatched.len(),
            samples: unmatched.into_iter().take(rules.sample_limit).collect(),
            sample_limit: rules.sample_limit,
        },
        non_claims: NON_CLAIMS
            .iter()
            .map(|claim| (*claim).to_string())
            .collect(),
    }
}

fn exact_rule_map(rules: &DeclaredPathProjectionRules) -> BTreeMap<&str, &DeclaredPathRule> {
    rules
        .exact_rules
        .iter()
        .map(|rule| (rule.raw_path.as_str(), rule))
        .collect()
}

fn map_exact(
    raw_path: &str,
    lookup_path: &str,
    operation: Option<&str>,
    exact_rules: &BTreeMap<&str, &DeclaredPathRule>,
) -> Option<PathProjectionMapping> {
    let rule = exact_rules.get(lookup_path)?;
    Some(PathProjectionMapping {
        raw_path: raw_path.to_string(),
        projected_path: with_operation(operation, &rule.projected_path),
        path_class: rule.path_class.clone(),
        relation: rule.relation.clone(),
        rule: rule.rule.clone(),
        confidence: CONFIDENCE_DECLARED.to_string(),
        claim_level: CLAIM_LEVEL_PROJECTED_EQUIVALENT.to_string(),
    })
}

fn map_workdir_prefix(
    raw_path: &str,
    lookup_path: &str,
    operation: Option<&str>,
    rules: &DeclaredPathProjectionRules,
) -> Option<PathProjectionMapping> {
    let relative = rules
        .workdir_prefixes
        .iter()
        .filter_map(|prefix| {
            strip_declared_prefix(lookup_path, prefix).map(|relative| (prefix.len(), relative))
        })
        .max_by_key(|(prefix_len, _)| *prefix_len)?
        .1;

    let projected_path = if relative.is_empty() {
        "workdir/".to_string()
    } else {
        format!("workdir/{relative}")
    };

    Some(PathProjectionMapping {
        raw_path: raw_path.to_string(),
        projected_path: with_operation(operation, &projected_path),
        path_class: PATH_CLASS_WORKLOAD_FIXTURE.to_string(),
        relation: RELATION_INSIDE_RUN_WORKDIR.to_string(),
        rule: RULE_DECLARED_WORKDIR_PREFIX.to_string(),
        confidence: CONFIDENCE_DECLARED.to_string(),
        claim_level: CLAIM_LEVEL_PROJECTED_EQUIVALENT.to_string(),
    })
}

fn strip_declared_prefix<'a>(path: &'a str, prefix: &str) -> Option<&'a str> {
    if path == prefix {
        return Some("");
    }
    let boundary = format!("{prefix}/");
    path.strip_prefix(&boundary)
}

fn normalize_prefix(mut prefix: String) -> String {
    while prefix.len() > 1 && prefix.ends_with('/') {
        prefix.pop();
    }
    prefix
}

fn split_operation_path(value: &str) -> (Option<&str>, &str) {
    let Some((operation, suffix)) = value.split_once(':') else {
        return (None, value);
    };
    if suffix.starts_with('/') && is_supported_operation(operation) {
        (Some(operation), suffix)
    } else {
        (None, value)
    }
}

fn is_supported_operation(operation: &str) -> bool {
    matches!(
        operation,
        "read"
            | "write"
            | "read_write"
            | "create"
            | "truncate"
            | "append"
            | "open"
            | "open_read"
            | "open_write"
            | "open_read_write"
            | "open_create"
            | "open_truncate"
            | "open_append"
    )
}

fn with_operation(operation: Option<&str>, projected_path: &str) -> String {
    operation.map_or_else(
        || projected_path.to_string(),
        |operation| format!("{operation}:{projected_path}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(values: &[&str]) -> BTreeSet<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn declared_workload_paths_project_to_roles_without_rewriting_raw_set() {
        let raw_paths = set(&[
            "/tmp/run/workdir/input.txt",
            "/tmp/run/workdir/output.json",
            "/tmp/run/workdir/scratch",
        ]);
        let original = raw_paths.clone();

        let mut rules = DeclaredPathProjectionRules::new();
        rules
            .add_workload_input_path("/tmp/run/workdir/input.txt")
            .add_workload_output_path("/tmp/run/workdir/output.json")
            .add_workload_scratch_path("/tmp/run/workdir/scratch");

        let projection = project_filesystem_paths(&raw_paths, &rules);

        assert_eq!(raw_paths, original);
        assert_eq!(projection.schema, PATH_PROJECTION_SCHEMA);
        assert_eq!(projection.status, STATUS_APPLIED);
        assert_eq!(projection.claim_level, CLAIM_LEVEL_PROJECTED_EQUIVALENT);
        assert_eq!(projection.unmatched_summary.count, 0);
        assert_eq!(
            projection.projected_paths(),
            set(&["workdir/input", "workdir/output", "workdir/scratch"])
        );
        assert!(projection
            .mappings
            .iter()
            .all(|mapping| mapping.confidence == CONFIDENCE_DECLARED));
        assert_eq!(
            projection.rules,
            vec![
                RULE_WORKLOAD_INPUT.to_string(),
                RULE_WORKLOAD_OUTPUT.to_string(),
                RULE_WORKLOAD_SCRATCH.to_string()
            ]
        );
    }

    #[test]
    fn declared_workdir_prefix_projects_inside_paths_and_preserves_operation_prefixes() {
        let raw_paths = set(&[
            "open_read:/tmp/a/workdir/input.txt",
            "/tmp/a/workdir/cache/provider.tmp",
            "https:/tmp/a/workdir/not-an-operation",
            "/tmp/a/workdir-else/not-inside.txt",
        ]);
        let mut rules = DeclaredPathProjectionRules::new();
        rules.add_workdir_prefix("/tmp/a/workdir/");

        let projection = project_filesystem_paths(&raw_paths, &rules);

        assert_eq!(
            projection.projected_paths(),
            set(&["open_read:workdir/input.txt", "workdir/cache/provider.tmp"])
        );
        assert_eq!(projection.unmatched_summary.count, 2);
        assert_eq!(
            projection.unmatched_summary.samples,
            vec![
                "/tmp/a/workdir-else/not-inside.txt".to_string(),
                "https:/tmp/a/workdir/not-an-operation".to_string()
            ]
        );
        assert!(projection
            .mappings
            .iter()
            .all(|mapping| mapping.rule == RULE_DECLARED_WORKDIR_PREFIX));
    }

    #[test]
    fn exact_declared_rules_win_over_declared_workdir_prefixes() {
        let raw_paths = set(&["open_read:/tmp/run/workdir/input.txt"]);
        let mut rules = DeclaredPathProjectionRules::new();
        rules
            .add_workdir_prefix("/tmp/run/workdir")
            .add_workload_input_path("/tmp/run/workdir/input.txt");

        let projection = project_filesystem_paths(&raw_paths, &rules);

        assert_eq!(
            projection.projected_paths(),
            set(&["open_read:workdir/input"])
        );
        assert_eq!(projection.mappings[0].rule, RULE_WORKLOAD_INPUT);
        assert_eq!(projection.mappings[0].relation, RELATION_WORKLOAD_CONTRACT);
    }

    #[test]
    fn unknown_paths_are_summarized_not_failed_or_collapsed() {
        let raw_paths = set(&[
            "/tmp/unknown-a",
            "/tmp/unknown-b",
            "/tmp/unknown-c",
            "/tmp/unknown-d",
        ]);
        let rules = DeclaredPathProjectionRules::new().with_sample_limit(2);

        let projection = project_filesystem_paths(&raw_paths, &rules);

        assert!(projection.mappings.is_empty());
        assert_eq!(projection.claim_level, CLAIM_LEVEL_INCONCLUSIVE);
        assert_eq!(projection.unmatched_summary.path_class, PATH_CLASS_UNKNOWN);
        assert_eq!(projection.unmatched_summary.count, 4);
        assert_eq!(projection.unmatched_summary.sample_limit, 2);
        assert_eq!(
            projection.unmatched_summary.samples,
            vec!["/tmp/unknown-a".to_string(), "/tmp/unknown-b".to_string()]
        );
        assert!(projection
            .non_claims
            .contains(&"projection_unknowns_preserved".to_string()));
    }

    #[test]
    fn projection_is_deterministic_for_repeated_runs() {
        let raw_paths = set(&[
            "/tmp/run/workdir/z.txt",
            "/tmp/run/workdir/a.txt",
            "/tmp/run/workdir/input.txt",
        ]);
        let mut rules = DeclaredPathProjectionRules::new();
        rules
            .add_workdir_prefix("/tmp/run/workdir")
            .add_workload_input_path("/tmp/run/workdir/input.txt");

        let first = project_filesystem_paths(&raw_paths, &rules);
        let second = project_filesystem_paths(&raw_paths, &rules);

        assert_eq!(first, second);
        assert_eq!(
            first
                .mappings
                .iter()
                .map(|mapping| mapping.raw_path.as_str())
                .collect::<Vec<_>>(),
            vec![
                "/tmp/run/workdir/a.txt",
                "/tmp/run/workdir/input.txt",
                "/tmp/run/workdir/z.txt"
            ]
        );
    }

    #[test]
    fn different_raw_paths_can_share_a_declared_projected_role_without_equivalence_claim() {
        let base_paths = set(&["open_read:/tmp/base/workdir/fixture-input.txt"]);
        let head_paths = set(&["open_read:/tmp/head/workdir/fixture-input.txt"]);

        let mut base_rules = DeclaredPathProjectionRules::new();
        base_rules.add_workload_input_path("/tmp/base/workdir/fixture-input.txt");
        let mut head_rules = DeclaredPathProjectionRules::new();
        head_rules.add_workload_input_path("/tmp/head/workdir/fixture-input.txt");

        let base_projection = project_filesystem_paths(&base_paths, &base_rules);
        let head_projection = project_filesystem_paths(&head_paths, &head_rules);

        assert_ne!(base_paths, head_paths);
        assert_eq!(
            base_projection.projected_paths(),
            head_projection.projected_paths()
        );
        assert_eq!(
            base_projection.projected_paths(),
            set(&["open_read:workdir/input"])
        );
        assert!(base_projection
            .non_claims
            .contains(&"projection_no_semantic_workload_equivalence".to_string()));
    }
}
