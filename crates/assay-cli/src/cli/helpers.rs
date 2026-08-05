use crate::exit_codes;
use assay_core::errors::diagnostic::{exit_class, Diagnostic, ExitClass, Severity};
use std::path::{Path, PathBuf};

/// The CLI spelling of a diagnostic severity.
///
/// An unrecognized value becomes `error`, not `note`. The previous fallback was
/// the least severe level, so a misspelled or newly introduced severity dropped
/// out of the set `decide_exit` inspects and could turn a failing run into exit
/// 0 (#2025). The vocabulary itself lives in `assay-core` beside `Diagnostic`,
/// because a second copy of it had already drifted (#2033).
pub fn normalize_severity(s: &str) -> &'static str {
    Severity::parse(s)
        .map(Severity::as_cli_str)
        .unwrap_or("error")
}

pub fn infer_policy_path(assay_yaml: &Path) -> Option<PathBuf> {
    let s = std::fs::read_to_string(assay_yaml).ok()?;
    let doc: serde_yaml::Value = serde_yaml::from_str(&s).ok()?;
    let m = doc.as_mapping()?;
    let v = m.get(serde_yaml::Value::String("policy".into()))?;
    let p = v.as_str()?;
    Some(PathBuf::from(p))
}

/// The process exit code for a set of diagnostics.
///
/// The class of each code comes from the registry that defines it. This function used to infer the
/// class by matching code prefixes, which made a code's spelling load-bearing for exit semantics
/// and had already drifted: `E_TRACE_MISS` and `E_PATH_NOT_FOUND` describe one missing-trace
/// condition and exited 1 and 2 respectively.
pub fn decide_exit(diags: &[Diagnostic]) -> i32 {
    let mut saw_error = false;
    let mut saw_config = false;

    for d in diags {
        if normalize_severity(&d.severity) != "error" {
            continue;
        }
        saw_error = true;
        match exit_class(&d.code) {
            ExitClass::Config => saw_config = true,
            // An unregistered code is not evidence of a config fault. `assay_core::validate`
            // forwards policy-engine verdict codes verbatim (`E_ARG_SCHEMA` and friends), and those
            // describe a test outcome. Registering them is #2027; until then they land here, which
            // is the same exit code the prefix match gave them.
            ExitClass::Test | ExitClass::Unregistered => {}
        }
    }

    if !saw_error {
        return exit_codes::OK;
    }
    if saw_config {
        exit_codes::CONFIG_ERROR
    } else {
        exit_codes::TEST_FAILED
    }
}

#[cfg(test)]
mod decide_exit_tests {
    use super::*;
    use assay_core::errors::diagnostic::{codes, ERROR_EXIT_CLASSES};

    fn err(code: &str) -> Diagnostic {
        Diagnostic::new(code, "test")
    }

    /// The defect this function was rewritten for: one missing-trace condition is reported under
    /// two codes depending on which lookup found it, and they used to exit 1 and 2.
    #[test]
    fn missing_trace_exits_the_same_under_either_code() {
        assert_eq!(
            decide_exit(&[err(codes::E_TRACE_MISS)]),
            decide_exit(&[err(codes::E_PATH_NOT_FOUND)]),
        );
    }

    /// The four codes the prefix list missed. Each is declared an error in the registry and each
    /// used to exit 1.
    #[test]
    fn codes_the_prefix_list_missed_now_exit_config() {
        for code in [
            codes::E_TRACE_MISS,
            codes::E_TRACE_INVALID,
            codes::E_REPLAY_STRICT_MISSING,
            codes::E_EMB_DIMS,
        ] {
            assert_eq!(
                decide_exit(&[err(code)]),
                exit_codes::CONFIG_ERROR,
                "{code} is a registered error code and must not exit as a test failure",
            );
        }
    }

    #[test]
    fn every_registered_error_code_exits_config() {
        for (code, _) in ERROR_EXIT_CLASSES {
            assert_eq!(
                decide_exit(&[err(code)]),
                exit_codes::CONFIG_ERROR,
                "{code}",
            );
        }
    }

    /// Policy-engine verdict codes reach `validate` unregistered. They describe a test outcome, and
    /// this is the exit code they had before the rewrite.
    #[test]
    fn unregistered_code_exits_test_failure() {
        assert_eq!(decide_exit(&[err("E_ARG_SCHEMA")]), exit_codes::TEST_FAILED);
        assert_eq!(decide_exit(&[err("E_UNKNOWN")]), exit_codes::TEST_FAILED);
    }

    /// A config fault anywhere in the set decides, matching the previous `any` semantics.
    #[test]
    fn one_config_code_among_test_codes_decides() {
        let diags = vec![err("E_ARG_SCHEMA"), err(codes::E_CFG_PARSE)];
        assert_eq!(decide_exit(&diags), exit_codes::CONFIG_ERROR);
    }

    #[test]
    fn warnings_and_empty_sets_exit_ok() {
        assert_eq!(decide_exit(&[]), exit_codes::OK);
        let warn = err(codes::W_CFG_VACUOUS_EXPECTED).with_severity("warn");
        assert_eq!(decide_exit(&[warn]), exit_codes::OK);
    }

    /// `E_TRACE_SCHEMA` was one of the four prefixes and has never named a code. Nothing may depend
    /// on it, and a code that merely starts with a config-looking prefix is no longer special.
    #[test]
    fn spelling_no_longer_decides() {
        assert_eq!(
            decide_exit(&[err("E_TRACE_SCHEMA")]),
            exit_codes::TEST_FAILED
        );
        assert_eq!(
            decide_exit(&[err("E_CFG_INVENTED")]),
            exit_codes::TEST_FAILED
        );
    }
}

#[cfg(test)]
mod severity_exit_tests {
    use super::{decide_exit, normalize_severity};
    use assay_core::errors::diagnostic::Diagnostic;

    #[test]
    fn an_unknown_severity_does_not_silently_pass() {
        // Previously `normalize_severity` returned "note" here, `decide_exit`
        // saw no error, and the command exited 0 on a diagnostic nobody could
        // classify (#2025).
        assert_eq!(normalize_severity("cirtical"), "error");
        let diag = Diagnostic {
            code: "E_CFG_PARSE".into(),
            severity: "cirtical".into(),
            source: "test".into(),
            message: "unclassifiable".into(),
            context: serde_json::Value::Null,
            fix_steps: vec![],
        };
        assert_ne!(decide_exit(&[diag]), 0, "an unknown severity exited 0");
    }

    #[test]
    fn known_severities_are_unchanged() {
        assert_eq!(normalize_severity("Warning"), "warn");
        assert_eq!(normalize_severity("INFO"), "note");
        assert_eq!(normalize_severity("error"), "error");
    }
}
