use crate::errors::diagnostic::{codes, Diagnostic};
use crate::model::EvalConfig;
use crate::model::Policy;
use std::collections::{HashSet, HashMap};

pub fn analyze_policy_usage(
    _cfg: &EvalConfig,
    policies: &HashMap<String, Policy>,
    diags: &mut Vec<Diagnostic>,
) {
    // 1. Identify all tools used in traces (heuristic from Test Inputs if explicit tool usage is known)
    // Actually, traces are external. We can only analyse what's in the Policy file vs what's theoretically allowed.

    // Check 1: Unused Tools in Policy (Simplistic: Is tool mentioned in any test expectation?)
    // This is hard without loading all traces.
    // Better check: "Alias Shadowing"

    for (path, policy) in policies {
        // Alias Shadowing Check
        let mut tool_names: HashSet<String> = HashSet::new();
        if let Some(allow) = &policy.tools.allow {
            for t in allow { tool_names.insert(t.clone()); }
        }
        // Also check keys in arg_constraints (tools with schema)
        if let Some(constraints) = &policy.tools.arg_constraints {
            for t in constraints.keys() {
                tool_names.insert(t.clone());
            }
        }

        for (alias, targets) in &policy.aliases {
            if tool_names.contains(alias) {
                diags.push(
                    Diagnostic::new(
                         // Need a new code for this? reusing CONFIG_INVALID for now
                        codes::E_CFG_SCHEMA,
                        format!("Alias '{}' shadows an explicit tool name.", alias)
                    )
                    .with_severity("warn")
                    .with_source("doctor.policy_analysis")
                    .with_context(serde_json::json!({
                        "policy_file": path,
                        "alias": alias,
                        "targets": targets
                    }))
                    .with_fix_step(format!("Rename alias '{}' to avoid confusion.", alias))
                );
            }
        }
    }
}
