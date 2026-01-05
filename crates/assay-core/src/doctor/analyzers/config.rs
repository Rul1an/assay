use crate::config::path_resolver::PathResolver;
use crate::errors::diagnostic::{codes, Diagnostic};
use crate::model::EvalConfig;

pub fn analyze_config_integrity(
    cfg: &EvalConfig,
    resolver: &PathResolver,
    diags: &mut Vec<Diagnostic>,
) {
    for test in &cfg.tests {
        if let Some(path) = test.expected.get_policy_path() {
            let mut p_str = path.to_string();
            resolver.resolve_str(&mut p_str);
            let pb = std::path::PathBuf::from(p_str);
            if !pb.exists() {
                // SOTA: Provide JSON Patch to create file? Or just Path fix?
                diags.push(
                    Diagnostic::new(
                        codes::E_PATH_NOT_FOUND,
                        format!("Policy file referenced in test '{}' missing", test.id),
                    )
                    .with_source("doctor.config_integrity")
                    .with_context(serde_json::json!({ "path": pb, "test_id": test.id }))
                    .with_fix_step(format!("Create missing policy file: {}", pb.display())),
                );
            }
        }
    }
}
