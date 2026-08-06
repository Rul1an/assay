//! The reason-code inventory in `docs/architecture/REASON-CODE-VOCABULARIES.md` is checked here.
//!
//! Three vocabularies write to a SARIF `ruleId`, and nothing held them together: a code could be
//! added to one and the others would not notice. This test makes the inventory the record, and
//! makes updating it the way a new code gets added.
//!
//! It lives in `assay-cli` because that is the only crate depending on both `assay-core` and
//! `assay-evidence`.

use std::collections::BTreeSet;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read(rel: &str) -> String {
    let path = workspace_root().join(rel);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// The members recorded under a `<!-- machine-checked: NAME -->` marker in the inventory.
///
/// Anchored on an HTML comment rather than on a heading so the prose around it can be rewritten
/// without silently changing what is checked.
fn recorded(marker: &str) -> BTreeSet<String> {
    let doc = read("docs/architecture/REASON-CODE-VOCABULARIES.md");
    let needle = format!("<!-- machine-checked: {marker} -->");
    let after = doc
        .split_once(&needle)
        .unwrap_or_else(|| panic!("inventory has no `{needle}` marker"))
        .1;
    let block = after
        .split_once("```text\n")
        .unwrap_or_else(|| panic!("marker `{marker}` is not followed by a ```text block"))
        .1;
    let block = block
        .split_once("\n```")
        .unwrap_or_else(|| panic!("marker `{marker}` block is unterminated"))
        .0;
    block
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

/// Every `pub const` in `assay_core::errors::diagnostic::codes`, by its *value*.
///
/// Parsed from source rather than read from a list in the crate, because a list is exactly the
/// thing that drifts: a `pub const` added beside it would not appear, and the check would report
/// coverage it does not have.
fn diagnostic_codes() -> BTreeSet<String> {
    let src = read("crates/assay-core/src/errors/diagnostic.rs");
    let body = module_body(&src, "pub mod codes {");
    let mut found = BTreeSet::new();
    for line in body.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub const ") else {
            continue;
        };
        let value = rest
            .split_once('=')
            .and_then(|(_, v)| string_literal(v))
            .unwrap_or_else(|| panic!("`pub const` in `codes` is not a string literal: {line}"));
        assert!(
            found.insert(value.clone()),
            "duplicate code value {value:?} in `codes`"
        );
    }
    assert!(!found.is_empty(), "parsed no codes; the module shape moved");
    found
}

/// Every `reason_code:` value constructed in `assay_core::policy_engine`.
///
/// These reach `Diagnostic.code` verbatim (`validate/mod.rs:219`) and so become SARIF `ruleId`s,
/// which is why they belong in the inventory despite not being a registry.
///
/// A `reason_code:` whose value is not a string literal is a hard failure, not a skip. A parser
/// that quietly ignored a computed code would keep passing while the surface it claims to cover
/// grew, which is the failure mode this whole file exists to prevent.
fn policy_engine_codes() -> BTreeSet<String> {
    let src = read("crates/assay-core/src/policy_engine.rs");
    let mut found = BTreeSet::new();
    for line in src.lines() {
        let trimmed = line.trim();
        // Skip the struct field declaration and any doc comment mentioning the name.
        if trimmed.starts_with("//") || trimmed.starts_with("pub reason_code") {
            continue;
        }
        let Some(rest) = trimmed.strip_prefix("reason_code:") else {
            continue;
        };
        let value = string_literal(rest).unwrap_or_else(|| {
            panic!(
                "policy_engine `reason_code:` is not a string literal, so the inventory cannot \
                 cover it: {trimmed}"
            )
        });
        found.insert(value);
    }
    assert!(
        !found.is_empty(),
        "parsed no policy-engine reason codes; the construction shape moved"
    );
    found
}

/// The body of a module, from its opening line to the matching brace.
fn module_body<'a>(src: &'a str, header: &str) -> &'a str {
    let start = src
        .find(header)
        .unwrap_or_else(|| panic!("source has no `{header}`"))
        + header.len();
    let rest = &src[start..];
    let mut depth = 1usize;
    for (idx, ch) in rest.char_indices() {
        match ch {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    return &rest[..idx];
                }
            }
            _ => {}
        }
    }
    panic!("`{header}` is unterminated");
}

/// The first double-quoted literal in `s`, with no escape handling — reason codes have none, and
/// a literal containing one would fail the inventory comparison rather than pass silently.
fn string_literal(s: &str) -> Option<String> {
    let start = s.find('"')? + 1;
    let end = s[start..].find('"')? + start;
    Some(s[start..end].to_owned())
}

/// Every `pub const` string in `assay_core::report::exercised`.
///
/// These reach the `warnings` array of `run.json` / `summary.json`, which is a different field from
/// `reason_code` and a different surface from a SARIF `ruleId`. Parsed the same way as `codes::` and
/// for the same reason: a second `pub const` added beside the first would otherwise reach a
/// published artifact with nothing recording it.
fn run_json_warning_codes() -> BTreeSet<String> {
    let src = read("crates/assay-core/src/report/exercised.rs");
    let mut found = BTreeSet::new();
    for line in src.lines() {
        let line = line.trim();
        let Some(rest) = line.strip_prefix("pub const ") else {
            continue;
        };
        let Some((_, value)) = rest.split_once('=') else {
            continue;
        };
        // Only string-valued constants are codes. A numeric bound is not a vocabulary member, and
        // treating one as a missing entry would make this check noise.
        if let Some(value) = string_literal(value) {
            assert!(
                found.insert(value.clone()),
                "duplicate warning code value {value:?}"
            );
        }
    }
    assert!(
        !found.is_empty(),
        "parsed no run.json warning codes; the module shape moved"
    );
    found
}

fn lint_rule_ids() -> BTreeSet<String> {
    assay_evidence::lint::rules::RULES
        .iter()
        .map(|rule| rule.id.to_owned())
        .collect()
}

#[test]
fn inventory_records_every_diagnostic_code() {
    assert_eq!(
        diagnostic_codes(),
        recorded("diagnostic-codes"),
        "`assay_core::errors::diagnostic::codes` and the inventory disagree. A code that reaches \
         a SARIF ruleId must be recorded in docs/architecture/REASON-CODE-VOCABULARIES.md, and \
         recording it is where the decision about the other vocabularies on that surface goes."
    );
}

#[test]
fn inventory_records_every_policy_engine_code() {
    assert_eq!(
        policy_engine_codes(),
        recorded("policy-engine-codes"),
        "`assay_core::policy_engine` verdict codes and the inventory disagree. These are forwarded \
         verbatim into `Diagnostic::new` at validate/mod.rs:219 and become SARIF ruleIds under the \
         same tool driver as `codes::`, so a new one is a new public id."
    );
}

#[test]
fn inventory_records_every_lint_rule_id() {
    assert_eq!(
        lint_rule_ids(),
        recorded("lint-rule-ids"),
        "`assay_evidence::lint::rules::RULES` and the inventory disagree."
    );
}

#[test]
fn inventory_records_every_run_json_warning_code() {
    assert_eq!(
        run_json_warning_codes(),
        recorded("run-json-warning-codes"),
        "`assay_core::report::exercised` and the inventory disagree. A code reaching the \
         `warnings` array of run.json is a published id and belongs in \
         docs/architecture/REASON-CODE-VOCABULARIES.md."
    );
}

/// The `warnings` codes are deliberately not in `codes::`, and that must stay a decision.
///
/// `codes::` is inventoried as the SARIF `ruleId` vocabulary under `tool.driver.name = "assay"`.
/// The `run` path does build `Diagnostic`s — the trace client, the assertion matchers, the pipeline
/// error classifier — but none of them reaches `build_sarif_diagnostics`, whose only non-test caller
/// is `assay validate --format sarif`. So a code moved into that registry to tidy it up would be
/// recorded on a surface it never reaches: the inventory would state something false, and this file
/// is the reason the inventory is trusted. If a run-path diagnostic ever gains a route to
/// `build_sarif_diagnostics`, move the constant and delete this test in the same change.
#[test]
fn the_run_json_warning_codes_are_not_in_the_sarif_registry() {
    let warnings = run_json_warning_codes();
    let sarif = diagnostic_codes();
    let overlap: Vec<&String> = warnings.intersection(&sarif).collect();
    assert!(
        overlap.is_empty(),
        "these codes are in both `report::exercised` and `codes::`, so the inventory records them \
         on a SARIF surface they do not reach: {overlap:?}"
    );
}

#[test]
fn the_two_sarif_rule_id_spaces_stay_disjoint() {
    let assay_driver: BTreeSet<String> = diagnostic_codes()
        .union(&policy_engine_codes())
        .cloned()
        .collect();
    let lint_driver = lint_rule_ids();
    let overlap: Vec<&String> = assay_driver.intersection(&lint_driver).collect();

    // They are separate namespaces today, because the driver names differ ("assay" against
    // "assay-evidence-lint"). Asserted anyway so that unifying the driver names is a decision
    // someone makes, not a collision someone finds afterwards in Code Scanning.
    assert!(
        overlap.is_empty(),
        "these ids exist in both SARIF ruleId vocabularies: {overlap:?}"
    );
}

#[test]
fn the_generic_test_result_rule_id_shadows_nothing() {
    // `write_sarif` emits `"ruleId": "assay"` for every test result, under the same
    // `tool.driver.name` that `build_sarif_diagnostics` uses. A reason code spelled `assay` would
    // merge with the entire test-result stream in one Code Scanning alert.
    const GENERIC: &str = "assay";
    assert!(
        !diagnostic_codes().contains(GENERIC),
        "a `codes::` member is spelled {GENERIC:?}, which is the generic test-result ruleId"
    );
    assert!(
        !policy_engine_codes().contains(GENERIC),
        "a policy-engine reason code is spelled {GENERIC:?}, which is the generic test-result ruleId"
    );
    assert!(
        !lint_rule_ids().contains(GENERIC),
        "a lint rule id is spelled {GENERIC:?}, which is the generic test-result ruleId"
    );
}
