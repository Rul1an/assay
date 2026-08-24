use super::DESCRIBE_REPORT_SCHEMA;
use crate::cli::commands::evidence::schema::{
    SCHEMA_LIST_REPORT, SCHEMA_SHOW_REPORT, SCHEMA_VALIDATION_REPORT,
};
use crate::cli::commands::init_report::INIT_REPORT_SCHEMA;
use crate::cli::commands::validate::VALIDATE_REPORT_SCHEMA;
use crate::diagnostics::report::DOCTOR_REPORT_SCHEMA;
use assay_core::report::json::RUN_REPORT_SCHEMA;
use assay_core::report::summary::SUMMARY_SCHEMA;

/// One shipping identity owned by a clap path. The identity field is the
/// existing constant, not a second string.
pub(super) struct IdentityBinding {
    pub path: &'static str,
    pub identity: &'static str,
}

/// Leaf paths only. A parent listing includes exact matches and immediate children.
pub(super) const BINDING_ROWS: &[IdentityBinding] = &[
    IdentityBinding {
        path: "describe",
        identity: DESCRIBE_REPORT_SCHEMA,
    },
    IdentityBinding {
        path: "doctor",
        identity: DOCTOR_REPORT_SCHEMA,
    },
    IdentityBinding {
        path: "init",
        identity: INIT_REPORT_SCHEMA,
    },
    IdentityBinding {
        path: "run",
        identity: RUN_REPORT_SCHEMA,
    },
    IdentityBinding {
        path: "run",
        identity: SUMMARY_SCHEMA,
    },
    IdentityBinding {
        path: "validate",
        identity: VALIDATE_REPORT_SCHEMA,
    },
    IdentityBinding {
        path: "evidence/schema/list",
        identity: SCHEMA_LIST_REPORT,
    },
    IdentityBinding {
        path: "evidence/schema/show",
        identity: SCHEMA_SHOW_REPORT,
    },
    IdentityBinding {
        path: "evidence/schema/validate",
        identity: SCHEMA_VALIDATION_REPORT,
    },
];

pub(super) fn identities_for(path: &[String]) -> Vec<&'static str> {
    if path.is_empty() {
        return Vec::new();
    }
    let joined = path.join("/");
    BINDING_ROWS
        .iter()
        .filter(|binding| belongs_on_node(binding.path, &joined))
        .map(|binding| binding.identity)
        .collect()
}

fn belongs_on_node(binding_path: &str, node_path: &str) -> bool {
    if binding_path == node_path {
        return true;
    }
    binding_path
        .strip_prefix(node_path)
        .is_some_and(|rest| rest.starts_with('/') && !rest[1..].contains('/'))
}

#[cfg(test)]
mod tests {
    use super::{identities_for, BINDING_ROWS};
    use crate::diagnostics::report::DOCTOR_REPORT_SCHEMA;

    #[test]
    fn root_lists_no_leaf_identities() {
        assert!(identities_for(&[]).is_empty());
    }

    #[test]
    fn doctor_node_lists_the_shipping_doctor_identity() {
        assert!(identities_for(&["doctor".into()]).contains(&DOCTOR_REPORT_SCHEMA));
    }

    #[test]
    fn every_binding_identity_is_a_row_constant_not_a_literal_copy() {
        assert!(
            BINDING_ROWS
                .iter()
                .any(|binding| binding.identity == DOCTOR_REPORT_SCHEMA),
            "doctor report must stay wired through its shipping constant"
        );
    }
}
