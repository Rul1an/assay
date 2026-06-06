use super::*;
use std::collections::BTreeSet;

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
