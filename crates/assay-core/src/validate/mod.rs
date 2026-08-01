use crate::config::path_resolver::PathResolver;
use crate::errors::diagnostic::{codes, Diagnostic};
use crate::model::EvalConfig;
use crate::model::Expected;
use crate::providers::llm::LlmClient; // Import trait for .complete()
use crate::providers::trace::TraceClient;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ValidateOptions {
    pub trace_file: Option<PathBuf>,
    pub baseline_file: Option<PathBuf>,
    pub replay_strict: bool,
}

#[derive(Debug, Clone, Default)]
pub struct ValidateReport {
    pub diagnostics: Vec<Diagnostic>,
}

pub async fn validate(
    cfg: &EvalConfig,
    opts: &ValidateOptions,
    resolver: &PathResolver,
) -> anyhow::Result<ValidateReport> {
    let mut diags = Vec::new();

    // 1. Path Resolution Checks (E_PATH_NOT_FOUND)
    // Actually the CLI loader does this, but we can double check config assets if any.
    // For now, let's assume config is loaded correctly if we are here,
    // but check the explicitly provided trace/baseline files if they exist.

    if let Some(path) = &opts.trace_file {
        if !path.exists() {
            diags.push(
                Diagnostic::new(
                    codes::E_PATH_NOT_FOUND,
                    format!("Trace file not found: {}", path.display()),
                )
                .with_context(serde_json::json!({ "path": path }))
                .with_source("validate")
                .with_fix_step("Ensure the --trace-file path is correct and accessible"),
            );
        }
    }

    if let Some(path) = &opts.baseline_file {
        if !path.exists() {
            diags.push(
                Diagnostic::new(
                    codes::E_PATH_NOT_FOUND,
                    format!("Baseline file not found: {}", path.display()),
                )
                .with_context(serde_json::json!({ "path": path }))
                .with_source("validate")
                .with_fix_step("Ensure the --baseline path is correct and accessible"),
            );
        }
    }

    // Return early if basic files missing to avoid noise. The vacuous-expected scan is
    // still reported: it is a pure config check that needs neither trace nor baseline.
    if !diags.is_empty() {
        diags.extend(check_vacuous_expected(cfg));
        return Ok(ValidateReport { diagnostics: diags });
    }

    // 1b. Vacuous assertions (E_CFG_VACUOUS_EXPECTED)
    diags.extend(check_vacuous_expected(cfg));

    // 2. Load Trace & Baseline for deeper checks
    let trace_client = if let Some(path) = &opts.trace_file {
        match TraceClient::from_path(path) {
            Ok(client) => Some(client),
            Err(e) => {
                diags.push(
                    Diagnostic::new(
                        codes::E_TRACE_INVALID,
                        format!("Failed to parse trace file: {}", e),
                    )
                    .with_source("trace")
                    .with_context(serde_json::json!({ "path": path, "error": e.to_string() })),
                );
                return Ok(ValidateReport { diagnostics: diags });
            }
        }
    } else {
        None
    };

    let baseline = if let Some(path) = &opts.baseline_file {
        match crate::baseline::Baseline::load(path) {
            Ok(b) => Some(b),
            Err(e) => {
                diags.push(
                    Diagnostic::new(
                        codes::E_BASE_MISMATCH,
                        format!("Failed to parse baseline: {}", e),
                    )
                    .with_source("baseline")
                    .with_context(serde_json::json!({ "path": path, "error": e.to_string() })),
                );
                return Ok(ValidateReport { diagnostics: diags });
            }
        }
    } else {
        None
    };

    // 3. Trace Coverage (E_TRACE_MISS)
    if let Some(client) = &trace_client {
        for tc in &cfg.tests {
            // We use the same lookup logic as TraceClient::complete
            // But here we want to collect ALL misses, not just fail on first.
            // Since `complete` is not exposed as "check only", we iterate.
            // Actually TraceClient doesn't expose keys publicly yet.
            // We might need to call complete and catch error?
            // OR better: call complete() on client. Since it returns LlmResponse or Err(Diagnostic)

            let res = client
                .complete(&tc.input.prompt, tc.input.context.as_deref())
                .await;
            if let Err(e) = res {
                // If it's a diagnostic, push it.
                // We use try_map_error from errors module
                if let Some(diag) = crate::errors::try_map_error(&e) {
                    // Enrich with test_id
                    let mut d = diag.clone();
                    if let serde_json::Value::Object(ref mut map) = d.context {
                        map.insert("test_id".into(), serde_json::json!(tc.id));
                        map.insert("trace_file".into(), serde_json::json!(opts.trace_file));
                    }
                    d.source = "trace".to_string();
                    diags.push(d);
                } else {
                    // Unexpected error?
                    diags.push(
                        Diagnostic::new("E_UNKNOWN", format!("Unexpected trace error: {}", e))
                            .with_source("trace"),
                    );
                }
            } else if let Ok(resp) = res {
                // Check Strict Replay (Requirement 4)
                if opts.replay_strict {
                    validate_strict_requirements(tc, &resp, &mut diags, opts.trace_file.as_deref());
                }

                // Check Embedding Dims (Requirement 5)
                // This is checking per-test, potentially spammy.
                // Better to check once per trace? But we don't have access to all embeddings.
                // We'll check via response meta if available.
                check_embedding_dims(&resp, &mut diags, opts.trace_file.as_deref());

                // Check Policy (Requirement 2: ArgsValid)
                if let Expected::ArgsValid {
                    policy: Some(policy_path),
                    ..
                } = &tc.expected
                {
                    // 1. Load Policy
                    // For now, load fully. In future, cache via resolver.
                    // We need to resolve relative to config?
                    // resolver.resolve_path(policy_path)?
                    let mut p_str = policy_path.clone();
                    resolver.resolve_str(&mut p_str);
                    let policy_file = std::path::PathBuf::from(p_str);
                    if !policy_file.exists() {
                        diags.push(
                            Diagnostic::new(
                                codes::E_PATH_NOT_FOUND,
                                format!("Policy file not found: {}", policy_file.display()),
                            )
                            .with_source("validate")
                            .with_context(serde_json::json!({ "path": policy_file })),
                        );
                    } else {
                        match crate::model::Policy::load(&policy_file) {
                            Ok(pol) => {
                                // 2. Get Tool Calls from Trace
                                let tool_calls =
                                    resp.meta.get("tool_calls").and_then(|v| v.as_array());

                                if let Some(calls) = tool_calls {
                                    // Convert to policy value for engine
                                    let policy_val = serde_json::to_value(
                                        pol.tools.arg_constraints.unwrap_or_default(),
                                    )
                                    .unwrap_or(serde_json::Value::Null);

                                    // Check for Allowed/Denied lists first?
                                    // Let's use simple policy_engine:evaluate_tool_args which expects JSON schema map.
                                    // Wait, Policy struct has complex structure.
                                    // policy.tools.arg_constraints is Map<Tool, Schema>.
                                    // policy.tools.allow/deny are lists.

                                    // Simplified validation for v1.2.1: Just check args against schema if present.
                                    // TODO(validate-v13): full policy context for arg enforcement

                                    for call in calls {
                                        let tool_name = call
                                            .get("tool_name")
                                            .and_then(|s| s.as_str())
                                            .unwrap_or("unknown");
                                        let args =
                                            call.get("args").unwrap_or(&serde_json::Value::Null);

                                        // Need to construct the "policy" value expected by evaluate_tool_args
                                        // It expects { "ToolName": Schema, ... }
                                        // This is exactly `arg_constraints`.

                                        let verdict = crate::policy_engine::evaluate_tool_args(
                                            &policy_val,
                                            tool_name,
                                            args,
                                        );

                                        if let crate::policy_engine::VerdictStatus::Blocked =
                                            verdict.status
                                        {
                                            let mut d = Diagnostic::new(
                                                verdict.reason_code,
                                                "Policy violation in tool call",
                                            )
                                            .with_source("policy")
                                            .with_context(verdict.details);

                                            // Add trace context
                                            if let serde_json::Value::Object(ref mut map) =
                                                d.context
                                            {
                                                map.insert("tool".into(), tool_name.into());
                                                map.insert("test_id".into(), tc.id.clone().into());
                                            }
                                            diags.push(d);
                                        }
                                    }
                                } else {
                                    // No tool calls found in trace?
                                    // If policy expects validation, maybe warn?
                                }
                            }
                            Err(e) => {
                                diags.push(
                                    Diagnostic::new(
                                        codes::E_CFG_PARSE,
                                        format!("Failed to parse policy: {}", e),
                                    )
                                    .with_source("policy"),
                                );
                            }
                        }
                    }
                }
            }
        }
    }

    // Baseline Compat (Requirement 3)
    if let Some(base) = &baseline {
        if base.suite != cfg.suite {
            diags.push(
                Diagnostic::new(codes::E_BASE_MISMATCH, "Baseline suite mismatch")
                    .with_source("baseline")
                    .with_context(serde_json::json!({
                        "expected_suite": cfg.suite,
                        "baseline_suite": base.suite,
                        "baseline_file": opts.baseline_file
                    }))
                    .with_fix_step("Use the baseline file created for this suite")
                    .with_fix_step("Or export a new baseline: assay ci ... --export-baseline ..."),
            );
        }
    }

    // Deduplicate diagnostics?
    // E_EMB_DIMS might be spammy if every test fails.
    // Simple dedup by code + message signature could be added later.

    Ok(ValidateReport { diagnostics: diags })
}

/// Flag tests whose `expected:` block asserts nothing and therefore always passes.
///
/// An empty `must_contain` / `must_not_contain` gives the metric no substring to look
/// for, so it returns `pass(1.0)` for any response whatsoever. The same shape is what a
/// test with no `expected:` key at all resolves to, via `Expected::default()`.
///
/// Tests that carry `assertions:` are exempt: for those, omitting `expected:` is the
/// documented way to let trace assertions do the checking, and the empty default is
/// then a placeholder rather than a claim.
///
/// This check reads only the config, so `assay validate` can sweep a suite for
/// always-green tests without running it.
fn check_vacuous_expected(cfg: &EvalConfig) -> Vec<Diagnostic> {
    let mut diags = Vec::new();

    for tc in &cfg.tests {
        let has_assertions = tc.assertions.as_ref().is_some_and(|a| !a.is_empty());
        if has_assertions {
            continue;
        }

        let field = match &tc.expected {
            Expected::MustContain { must_contain } if must_contain.is_empty() => "must_contain",
            Expected::MustNotContain { must_not_contain } if must_not_contain.is_empty() => {
                "must_not_contain"
            }
            _ => continue,
        };

        diags.push(
            Diagnostic::new(
                codes::E_CFG_VACUOUS_EXPECTED,
                format!(
                    "Test '{}' asserts nothing: `{}` is empty, so this test passes for any response",
                    tc.id, field
                ),
            )
            .with_source("config")
            .with_context(serde_json::json!({
                "test_id": tc.id,
                "field": field,
            }))
            .with_fix_step(format!("Give `{}` at least one entry", field))
            .with_fix_step("Or replace the expected block with a metric that checks something")
            .with_fix_step("Or move the test's checks to `assertions:`"),
        );
    }

    diags
}

fn validate_strict_requirements(
    tc: &crate::model::TestCase,
    resp: &crate::model::LlmResponse,
    diags: &mut Vec<Diagnostic>,
    trace_path: Option<&Path>,
) {
    let mut missing = Vec::new();

    // Check Semantic Metrics -> Need Embeddings
    if let Expected::SemanticSimilarityTo { .. } = &tc.expected {
        if resp.meta.pointer("/assay/embeddings/response").is_none() {
            missing.push(serde_json::json!({
                "requirement": "embeddings",
                "needed_by": ["semantic_similarity_to"],
                "meta_path": "meta.assay.embeddings"
            }));
        }
    }

    // Check Judge -> Need Judge Results
    // Only if expected is Faithfulness or Relevance
    match &tc.expected {
        Expected::Faithfulness { .. }
            if resp.meta.pointer("/assay/judge/faithfulness").is_none() =>
        {
            missing.push(serde_json::json!({
                "requirement": "judge_faithfulness",
                "needed_by": ["faithfulness"],
                "meta_path": "meta.assay.judge.faithfulness"
            }));
        }
        Expected::Relevance { .. } if resp.meta.pointer("/assay/judge/relevance").is_none() => {
            missing.push(serde_json::json!({
                "requirement": "judge_relevance",
                "needed_by": ["relevance"],
                "meta_path": "meta.assay.judge.relevance"
            }));
        }
        _ => {}
    }

    if !missing.is_empty() {
        diags.push(
            Diagnostic::new(
                codes::E_REPLAY_STRICT_MISSING,
                "Strict replay requires precomputed data that is missing from trace",
            )
            .with_source("replay")
            .with_context(serde_json::json!({
                "replay_strict": true,
                "trace_file": trace_path,
                "missing": missing,
                "test_id": tc.id
            }))
            .with_fix_step("Run `assay trace precompute-embeddings ...`")
            .with_fix_step("Run `assay trace precompute-judge ...`"),
        );
    }
}

fn check_embedding_dims(
    resp: &crate::model::LlmResponse,
    diags: &mut Vec<Diagnostic>,
    trace_path: Option<&Path>,
) {
    // Basic heuristic: if we have embeddings, check simple consistency?
    // Or if we know expected model?
    // For now, looking for obvious bad data (empty vectors)
    // Or strict mismatch if we ever passed an embedder config (not available here yet).

    if let Some(embeddings) = resp
        .meta
        .pointer("/assay/embeddings")
        .and_then(|v| v.as_object())
    {
        if let Some(response_vec) = embeddings.get("response").and_then(|v| v.as_array()) {
            if response_vec.is_empty() {
                diags.push(
                    Diagnostic::new(codes::E_EMB_DIMS, "Empty embedding vector found in trace")
                        .with_source("trace")
                        .with_context(serde_json::json!({ "trace_file": trace_path }))
                        .with_fix_step("Regenerate embeddings with precompute-embeddings"),
                );
            }
        }
    }
}
#[cfg(test)]
mod vacuous_expected_tests {
    use super::*;
    use crate::agent_assertions::model::TraceAssertion;
    use crate::model::{Settings, TestCase, TestInput};

    fn cfg_with(expected: Expected, assertions: Option<Vec<TraceAssertion>>) -> EvalConfig {
        EvalConfig {
            version: 1,
            suite: "s".into(),
            model: "dummy".into(),
            settings: Settings::default(),
            thresholds: Default::default(),
            otel: Default::default(),
            tests: vec![TestCase {
                id: "t1".into(),
                input: TestInput {
                    prompt: "hi".into(),
                    context: None,
                },
                expected,
                assertions,
                on_error: None,
                tags: vec![],
                metadata: None,
            }],
        }
    }

    #[test]
    fn flags_empty_must_contain() {
        let cfg = cfg_with(
            Expected::MustContain {
                must_contain: vec![],
            },
            None,
        );
        let diags = check_vacuous_expected(&cfg);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].code, codes::E_CFG_VACUOUS_EXPECTED);
        assert_eq!(diags[0].severity, "error");
        assert!(diags[0].message.contains("t1"), "{}", diags[0].message);
    }

    #[test]
    fn flags_empty_must_not_contain() {
        let cfg = cfg_with(
            Expected::MustNotContain {
                must_not_contain: vec![],
            },
            None,
        );
        let diags = check_vacuous_expected(&cfg);
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].context["field"], "must_not_contain");
    }

    /// A missing `expected:` key resolves to the vacuous default, so the same rule
    /// covers it — this is what keeps the permissive parse honest.
    #[test]
    fn flags_default_expected_from_missing_key() {
        let cfg = cfg_with(Expected::default(), None);
        assert_eq!(check_vacuous_expected(&cfg).len(), 1);
    }

    #[test]
    fn does_not_flag_populated_must_contain() {
        let cfg = cfg_with(
            Expected::MustContain {
                must_contain: vec!["Paris".into()],
            },
            None,
        );
        assert!(check_vacuous_expected(&cfg).is_empty());
    }

    /// Assertion-carrying tests legitimately omit `expected:`.
    #[test]
    fn does_not_flag_when_assertions_present() {
        let cfg = cfg_with(
            Expected::default(),
            Some(vec![TraceAssertion::TraceMustCallTool {
                tool: "search".into(),
                min_calls: None,
            }]),
        );
        assert!(check_vacuous_expected(&cfg).is_empty());
    }

    /// The sweep must work with no trace file and no baseline — that is the point of
    /// being able to check a suite without running it.
    #[tokio::test]
    async fn validate_reports_vacuous_without_trace_file() {
        let cfg = cfg_with(
            Expected::MustContain {
                must_contain: vec![],
            },
            None,
        );
        let opts = ValidateOptions {
            trace_file: None,
            baseline_file: None,
            replay_strict: false,
        };
        let resolver = PathResolver::new(Path::new("eval.yaml"));

        let report = validate(&cfg, &opts, &resolver).await.expect("validate");
        assert_eq!(report.diagnostics.len(), 1);
        assert_eq!(report.diagnostics[0].code, codes::E_CFG_VACUOUS_EXPECTED);
    }
}
