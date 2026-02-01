use crate::exit_codes;
use assay_core::errors::diagnostic::Diagnostic;
use std::path::{Path, PathBuf};

pub fn normalize_severity(s: &str) -> &'static str {
    if s.eq_ignore_ascii_case("error") {
        return "error";
    }
    if s.eq_ignore_ascii_case("warn") || s.eq_ignore_ascii_case("warning") {
        return "warn";
    }
    if s.eq_ignore_ascii_case("note") || s.eq_ignore_ascii_case("info") {
        return "note";
    }
    "note"
}

pub fn infer_policy_path(assay_yaml: &Path) -> Option<PathBuf> {
    let s = std::fs::read_to_string(assay_yaml).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&s).ok()?;
    let m = doc.as_mapping()?;
    let v = m.get(serde_yaml::Value::String("policy".into()))?;
    let p = v.as_str()?;
    Some(PathBuf::from(p))
}

pub fn decide_exit(diags: &[Diagnostic]) -> i32 {
    let has_error = diags
        .iter()
        .any(|d| normalize_severity(&d.severity) == "error");
    if !has_error {
        return exit_codes::OK;
    }

    let is_config_like = diags.iter().any(|d| {
        normalize_severity(&d.severity) == "error"
            && (d.code.starts_with("E_CFG_")
                || d.code.starts_with("E_PATH_")
                || d.code.starts_with("E_TRACE_SCHEMA")
                || d.code.starts_with("E_BASE_MISMATCH"))
    });

    if is_config_like {
        exit_codes::CONFIG_ERROR
    } else {
        exit_codes::TEST_FAILED
    }
}
