use serde_json::Value;
use std::cell::RefCell;
use std::collections::BTreeMap;

#[derive(Debug, PartialEq, Eq)]
pub enum WorkingDirectory {
    Invocation,
    SourceRepoRelative(Vec<String>),
}

pub fn classify_working_directory(step: &Value) -> Result<WorkingDirectory, String> {
    let Some(value) = step.get("working_directory") else {
        return Ok(WorkingDirectory::Invocation);
    };
    if value.is_null() {
        return Ok(WorkingDirectory::Invocation);
    }
    let path = value
        .as_str()
        .ok_or_else(|| "working_directory must be a POSIX-relative string".to_owned())?;
    if path.is_empty() || path.starts_with('/') || path.contains('\\') || has_drive_prefix(path) {
        return Err(format!(
            "working_directory must be a non-empty POSIX path relative to the source repo: {path:?}"
        ));
    }
    let components = path
        .split('/')
        .map(|component| {
            if component.is_empty() || matches!(component, "." | "..") {
                Err(format!(
                    "working_directory contains an unsafe path component: {path:?}"
                ))
            } else {
                Ok(component.to_owned())
            }
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(WorkingDirectory::SourceRepoRelative(components))
}

fn has_drive_prefix(path: &str) -> bool {
    let bytes = path.as_bytes();
    bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':'
}

thread_local! {
    static DRIVEN_OUTCOMES: RefCell<BTreeMap<(String, String), usize>> =
        const { RefCell::new(BTreeMap::new()) };
}

pub fn record_outcome(step_id: &str, outcome_name: &str) {
    DRIVEN_OUTCOMES.with(|driven| {
        *driven
            .borrow_mut()
            .entry((step_id.to_owned(), outcome_name.to_owned()))
            .or_default() += 1;
    });
}

pub fn assert_exact(contract: &Value, binary: &str, scenarios: &[fn()]) {
    DRIVEN_OUTCOMES.with(|driven| driven.borrow_mut().clear());
    for scenario in scenarios {
        scenario();
    }
    let driven = DRIVEN_OUTCOMES.with(|driven| driven.borrow().clone());
    assert_eq!(
        driven,
        expected_contract_outcomes(contract, binary),
        "runtime scenarios must drive every contract outcome exactly once"
    );
}

fn expected_contract_outcomes(contract: &Value, binary: &str) -> BTreeMap<(String, String), usize> {
    let mut expected = BTreeMap::new();
    for step in contract["steps"]
        .as_array()
        .expect("contract steps array")
        .iter()
        .filter(|step| step["binary"] == binary)
    {
        let step_id = step["id"].as_str().expect("contract step id string");
        for outcome in step["outcomes"].as_array().expect("step outcomes array") {
            let outcome_name = outcome["name"]
                .as_str()
                .expect("contract outcome name string");
            assert!(
                expected
                    .insert((step_id.to_owned(), outcome_name.to_owned()), 1)
                    .is_none(),
                "duplicate contract outcome {step_id}/{outcome_name}"
            );
        }
    }
    expected
}

#[cfg(test)]
mod working_directory_tests {
    use super::{classify_working_directory, WorkingDirectory};
    use serde_json::json;

    #[test]
    fn absent_working_directory_means_invocation_cwd() {
        assert_eq!(
            classify_working_directory(&json!({"id": "doctor"})),
            Ok(WorkingDirectory::Invocation)
        );
    }

    #[test]
    fn source_repo_relative_working_directory_is_split_into_safe_components() {
        assert_eq!(
            classify_working_directory(&json!({
                "id": "protected-action",
                "working_directory": "examples/privileged-action-gate"
            })),
            Ok(WorkingDirectory::SourceRepoRelative(vec![
                "examples".to_owned(),
                "privileged-action-gate".to_owned(),
            ]))
        );
    }

    #[test]
    fn hostile_or_ambiguous_working_directories_are_rejected() {
        for path in [
            "",
            "/tmp/assay",
            "C:/assay",
            "C:\\assay",
            "examples//privileged-action-gate",
            "./examples",
            "examples/.",
            "../examples",
            "examples/..",
            "examples\\privileged-action-gate",
        ] {
            let error = classify_working_directory(&json!({
                "id": "protected-action",
                "working_directory": path,
            }))
            .expect_err("unsafe working directory must be rejected");
            assert!(
                error.contains("working_directory"),
                "diagnostic for {path:?} did not name working_directory: {error}"
            );
        }
    }

    #[test]
    fn non_string_working_directory_is_rejected() {
        let error = classify_working_directory(&json!({
            "id": "protected-action",
            "working_directory": ["examples", "privileged-action-gate"],
        }))
        .expect_err("non-string working directory must be rejected");
        assert!(error.contains("working_directory"));
    }
}
