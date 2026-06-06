use std::collections::{BTreeMap, BTreeSet};

use super::{
    DeclaredPathProjectionRules, DeclaredPathRule, PathProjection, PathProjectionMapping,
    UnmatchedPathSummary, CLAIM_LEVEL_INCONCLUSIVE, CLAIM_LEVEL_PROJECTED_EQUIVALENT,
    CONFIDENCE_DECLARED, NON_CLAIMS, PATH_CLASS_UNKNOWN, PATH_CLASS_WORKLOAD_FIXTURE,
    PATH_PROJECTION_SCHEMA, RELATION_INSIDE_RUN_WORKDIR, RULE_DECLARED_WORKDIR_PREFIX,
    STATUS_APPLIED, TAXONOMY_SCHEMA,
};

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
