use crate::config::path_resolver::PathResolver;
use crate::errors::diagnostic::{codes, path_json, Diagnostic};
use crate::metrics_api::Metric;
use crate::model::EvalConfig;
use crate::model::Expected;
use crate::model::{LlmResponse, TestCase};
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

/// Validate a config against an optional trace and baseline.
///
/// This entry point does not decide `sequence_valid` or `args_valid` tests against the trace: the
/// evaluators `assay run` uses live in `assay-metrics`, which depends on this crate. It still binds
/// each such test and applies the execution contract `assay run` applies before a test runs. Use
/// [`validate_with_trace_metrics`] to decide them too, as `assay validate` does.
///
/// The resolver is not used. A `policy:` path is read as `assay run` reads it, relative to the
/// working directory.
pub async fn validate(
    cfg: &EvalConfig,
    opts: &ValidateOptions,
    _resolver: &PathResolver,
) -> anyhow::Result<ValidateReport> {
    validate_with(cfg, opts, None).await
}

/// The evaluators `assay run` decides trace-backed tests with, passed in because they live in
/// `assay-metrics`, which depends on this crate.
pub struct TraceMetrics<'a> {
    /// Decides each `sequence_valid` test (`assay run` uses `SequenceValidMetric`).
    pub sequence_valid: &'a dyn Metric,
    /// Decides each `args_valid` test (`assay run` uses `ArgsValidMetric`).
    pub args_valid: &'a dyn Metric,
}

/// [`validate`], and with a trace, each `sequence_valid` and `args_valid` test decided by the
/// metric in `metrics` for its kind.
///
/// Pass the metrics `assay run` evaluates these tests with, so validate reaches run's verdict
/// through the same code rather than a second reading of the rule language or the policy. A test
/// whose rules the trace breaks is reported as `E_SEQUENCE_VIOLATION`, and one whose argument
/// policy it breaks as `E_ARGS_VIOLATION`.
pub async fn validate_with_trace_metrics(
    cfg: &EvalConfig,
    opts: &ValidateOptions,
    metrics: &TraceMetrics<'_>,
) -> anyhow::Result<ValidateReport> {
    validate_with(cfg, opts, Some(metrics)).await
}

async fn validate_with(
    cfg: &EvalConfig,
    opts: &ValidateOptions,
    metrics: Option<&TraceMetrics<'_>>,
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
                .with_context(serde_json::json!({ "path": path_json(path) }))
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
                .with_context(serde_json::json!({ "path": path_json(path) }))
                .with_source("validate")
                .with_fix_step("Ensure the --baseline path is correct and accessible"),
            );
        }
    }

    // Missing path assets stop the deeper checks to avoid noise. The vacuous scan
    // still runs once because it needs neither trace nor baseline.
    let paths_missing = !diags.is_empty();
    diags.extend(check_vacuous_expected(cfg));
    let trace_checks = bind_trace_checks(cfg, &mut diags);
    if paths_missing {
        return Ok(ValidateReport { diagnostics: diags });
    }

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
                    .with_context(
                        serde_json::json!({ "path": path_json(path), "error": e.to_string() }),
                    ),
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
                    .with_context(
                        serde_json::json!({ "path": path_json(path), "error": e.to_string() }),
                    ),
                );
                return Ok(ValidateReport { diagnostics: diags });
            }
        }
    } else {
        None
    };

    // 3. Trace Coverage (E_TRACE_MISS)
    if let Some(client) = &trace_client {
        for (index, tc) in cfg.tests.iter().enumerate() {
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
                        map.insert(
                            "trace_file".into(),
                            opts.trace_file
                                .as_deref()
                                .map(path_json)
                                .unwrap_or(serde_json::Value::Null),
                        );
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

                if let (Some(metrics), Some(expected)) = (metrics, &trace_checks[index]) {
                    diags.extend(evaluate_trace_check(metrics, tc, expected, &resp).await);
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
                        "baseline_file": opts.baseline_file.as_deref().map(path_json),
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

/// Warn about tests that assert nothing and therefore always pass.
///
/// By the time a config has loaded, a vacuous value normally came from an omitted or
/// null `expected:` key resolving to `Expected::default()`. An explicit tagged
/// assertion that has no effective constraint is rejected at parse time (see
/// `model::serde::reject_vacuous`), which is a hard error for every command that
/// loads a config, including `assay run` and `assay ci`.
///
/// That split is deliberate. Omitting `expected:` is a documented, legitimate shape —
/// a test may carry its checks in `assertions:` — so making it an error here would
/// contradict the permissive parse and break configs the tool itself writes. It is
/// still worth reporting when such a test has no assertions either, because then it
/// really does assert nothing; hence a warning rather than an error.
///
/// Tests that carry `assertions:` are not exempt — they are swept too.
///
/// The exemption used to test for a **non-empty** `assertions:` list rather than an **effective**
/// one, and nothing looked at the assertions afterwards. One assertion that could not fail
/// therefore cleared both gates in a single move: the `expected:` check was skipped because
/// assertions existed, and the assertions were never examined. Reporting a suite as swept while
/// stepping over the case the sweep exists to find is the failure this function is meant to
/// prevent, one layer up.
///
/// Effectiveness is decided by `agent_assertions::matchers::ineffective_reason`, which is the same
/// code the evaluator runs. Nothing here re-states what "cannot fail" means.
///
/// This check reads only the config, so `assay validate` can sweep a suite for
/// always-green tests without running it.
/// Every assertion in the config that cannot fail, located in the suite.
///
/// Two callers need this answer and they must not each derive it: `assay validate` reports it as a
/// warning, and `load_config` refuses the config outright when the caller opted into
/// `allow_ineffective_assertions`. Two implementations of one rule drift; the fix is one
/// implementation with two callers, which is also why the "cannot fail" decision itself stays in
/// `agent_assertions::matchers::ineffective_reason` — the evaluator's own code — rather than being
/// restated here.
///
/// The diagnostics carry the evaluator's context, which names the variant and the responsible
/// field, plus `test_id` and `assertion_index`, which only a sweep over the config can supply.
pub fn ineffective_assertions(cfg: &EvalConfig) -> Vec<Diagnostic> {
    let mut diags = Vec::new();
    for tc in &cfg.tests {
        for (index, assertion) in tc
            .assertions
            .as_deref()
            .unwrap_or_default()
            .iter()
            .enumerate()
        {
            let Some(mut reason) = crate::agent_assertions::matchers::ineffective_reason(assertion)
            else {
                continue;
            };
            if let Some(obj) = reason.context.as_object_mut() {
                obj.insert("test_id".into(), serde_json::json!(tc.id));
                obj.insert("assertion_index".into(), serde_json::json!(index));
            }
            diags.push(
                reason.with_fix_step(
                    "Or remove the assertion, so the test does not appear to check it",
                ),
            );
        }
    }
    diags
}

/// The kinds of test validate decides against a trace, named as `assay run` names their metric.
fn trace_check_kind(e: &Expected) -> Option<&'static str> {
    if matches!(e, Expected::SequenceValid { .. }) {
        Some("sequence_valid")
    } else if matches!(e, Expected::ArgsValid { .. }) {
        Some("args_valid")
    } else {
        None
    }
}

/// Bind each `sequence_valid` and `args_valid` test the way `assay run` binds it before the test
/// runs, and apply the same execution contract, reporting a refusal as `E_CFG_PARSE`.
///
/// Returns the bound check per test, in config order, for the tests that passed, so the trace
/// check evaluates the snapshot the contract approved. A `policy:` file is read once here and is
/// resolved against the working directory, as `assay run` resolves it.
///
/// For `sequence_valid` this used to happen nowhere in validate, and for `args_valid` validate
/// read the file as a different policy than `assay run` reads, so files that run refuses validated
/// and files run enforces were refused. None of this needs a trace, so it runs without one.
fn bind_trace_checks(cfg: &EvalConfig, diags: &mut Vec<Diagnostic>) -> Vec<Option<Expected>> {
    cfg.tests
        .iter()
        .map(|tc| {
            let kind = trace_check_kind(&tc.expected)?;
            let mut bound = tc.expected.clone();
            let checked = crate::model::bind_external_expected_inputs(&mut bound)
                .and_then(|()| crate::model::validate_expected_for_execution(&bound));
            match checked {
                Ok(()) => Some(bound),
                Err(e) => {
                    diags.push(
                        Diagnostic::new(
                            codes::E_CFG_PARSE,
                            format!("Test '{}': invalid {kind} check: {e:#}", tc.id),
                        )
                        .with_source("policy")
                        .with_context(serde_json::json!({
                            "test_id": tc.id,
                            "policy": tc.expected.get_policy_path(),
                        })),
                    );
                    None
                }
            }
        })
        .collect()
}

/// Decide one bound test on the trace's response with the metric `assay run` uses for its kind.
/// A failed `sequence_valid` verdict is `E_SEQUENCE_VIOLATION` and a failed `args_valid` verdict
/// `E_ARGS_VIOLATION`, with the metric's record of each refusal in the context; a pass, including a rule or a policy no call exercised, is no diagnostic, as it
/// is no failure in `assay run`.
async fn evaluate_trace_check(
    metrics: &TraceMetrics<'_>,
    tc: &TestCase,
    expected: &Expected,
    resp: &LlmResponse,
) -> Option<Diagnostic> {
    let kind = trace_check_kind(expected)?;
    let (metric, code, source, fix) = if kind == "sequence_valid" {
        (
            metrics.sequence_valid,
            codes::E_SEQUENCE_VIOLATION,
            "sequence",
            "Run `assay run` with this trace for the per-rule record",
        )
    } else {
        (
            metrics.args_valid,
            codes::E_ARGS_VIOLATION,
            "policy",
            "Run `assay run` with this trace for the per-call record",
        )
    };
    match metric.evaluate(tc, expected, resp).await {
        Ok(result) if result.passed => None,
        Ok(result) => {
            let reason = result
                .details
                .get("message")
                .and_then(serde_json::Value::as_str)
                .map(str::to_string)
                .unwrap_or_else(|| format!("{kind} failed"));
            Some(
                Diagnostic::new(code, format!("Test '{}': {reason}", tc.id))
                    .with_source(source)
                    .with_context(serde_json::json!({
                        "test_id": tc.id,
                        "details": result.details,
                    }))
                    .with_fix_step(fix),
            )
        }
        Err(e) => Some(
            Diagnostic::new(
                codes::E_CFG_PARSE,
                format!("Test '{}': {kind} could not be evaluated: {e:#}", tc.id),
            )
            .with_source(source)
            .with_context(serde_json::json!({ "test_id": tc.id })),
        ),
    }
}

fn check_vacuous_expected(cfg: &EvalConfig) -> Vec<Diagnostic> {
    // An assertion that cannot fail is reported here rather than only when a run reaches it,
    // so a suite can be swept for always-green tests without executing anything.
    let mut diags = ineffective_assertions(cfg);

    for tc in &cfg.tests {
        let has_assertions = !tc.assertions.as_deref().unwrap_or_default().is_empty();

        if has_assertions {
            continue;
        }

        let Some(field) = crate::model::vacuous_expected_field(&tc.expected) else {
            continue;
        };

        diags.push(
            Diagnostic::new(
                codes::W_CFG_VACUOUS_EXPECTED,
                format!(
                    "Test '{}' asserts nothing: `{}` is empty and there are no `assertions:`, so it passes for any response",
                    tc.id, field
                ),
            )
            .with_severity("warn")
            .with_source("config")
            .with_context(serde_json::json!({
                "test_id": tc.id,
                "field": field,
            }))
            .with_fix_step("Add an `expected:` block that checks something")
            .with_fix_step("Or give the test `assertions:`"),
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
    #[expect(
        clippy::wildcard_enum_match_arm,
        reason = "only judge variants require judge meta; a new one would report no missing requirement"
    )]
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
                "trace_file": trace_path.map(path_json),
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
                        .with_context(
                            serde_json::json!({ "trace_file": trace_path.map(path_json) }),
                        )
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
        assert_eq!(diags[0].code, codes::W_CFG_VACUOUS_EXPECTED);
        // Warning, not error: omitted or null `expected:` values resolve to the
        // default, while an explicitly tagged empty assertion never gets this far.
        assert_eq!(diags[0].severity, "warn");
        assert!(diags[0].message.contains("t1"), "{}", diags[0].message);
        assert!(
            diags[0].message.contains("`must_contain` is empty"),
            "{}",
            diags[0].message
        );
        assert!(!diags[0].message.contains("no `expected:` block"));
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

    /// The case the exemption used to step over: a test carrying one assertion that cannot fail.
    ///
    /// Before this check, the non-empty `assertions:` list suppressed the `expected:` warning and
    /// nothing looked at the assertion, so the suite swept clean while asserting nothing.
    #[test]
    fn flags_an_assertion_that_cannot_fail() {
        let cfg = cfg_with(
            Expected::default(),
            Some(vec![TraceAssertion::TraceMustCallTool {
                tool: "search".into(),
                min_calls: Some(0),
            }]),
        );
        let diags = check_vacuous_expected(&cfg);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, "E_ASSERT_INEFFECTIVE");
        assert_eq!(diags[0].severity, "error");
        assert_eq!(diags[0].context["field"], "min_calls");
        assert_eq!(diags[0].context["test_id"], "t1");
        assert_eq!(diags[0].context["assertion_index"], 0);
    }

    /// One per variant. A sweep that reached only the variants convenient to write would be the
    /// same partial-coverage problem it exists to report.
    #[test]
    fn flags_a_vacuous_shape_of_every_variant() {
        let cases: Vec<(&str, TraceAssertion)> = vec![
            (
                "tool",
                TraceAssertion::TraceMustCallTool {
                    tool: String::new(),
                    min_calls: None,
                },
            ),
            (
                "tool",
                TraceAssertion::TraceMustNotCallTool {
                    tool: String::new(),
                },
            ),
            (
                "sequence",
                TraceAssertion::TraceToolSequence {
                    sequence: vec![],
                    allow_other_tools: true,
                },
            ),
            ("max", TraceAssertion::TraceMaxSteps { max: u32::MAX }),
            (
                "test_args",
                TraceAssertion::ArgsValid {
                    tool: "t".into(),
                    test_args: None,
                    policy: None,
                    expect: None,
                },
            ),
            (
                "test_trace_raw",
                TraceAssertion::SequenceValid {
                    test_trace: None,
                    test_trace_raw: None,
                    policy: None,
                    expect: None,
                },
            ),
            (
                "test_tool_calls",
                TraceAssertion::ToolBlocklist {
                    test_tool_calls: None,
                    policy: None,
                    expect: None,
                },
            ),
        ];

        for (field, assertion) in cases {
            let cfg = cfg_with(Expected::default(), Some(vec![assertion.clone()]));
            let diags = check_vacuous_expected(&cfg);
            assert_eq!(diags.len(), 1, "{assertion:?} produced {diags:?}");
            assert_eq!(
                diags[0].context["field"], field,
                "{assertion:?} blamed the wrong field: {}",
                diags[0].message
            );
        }
    }

    /// An unrecognized `expect` spelling silently selected *expect failure* and inverted the
    /// assertion. That is worse than a no-op — a no-op stops checking, an inversion checks the
    /// opposite — so the static sweep has to reach it too, not only the evaluator.
    #[test]
    fn flags_an_unrecognized_expect_spelling() {
        let cfg = cfg_with(
            Expected::default(),
            Some(vec![TraceAssertion::ArgsValid {
                tool: "t".into(),
                test_args: Some(serde_json::json!({})),
                policy: Some(serde_json::json!({ "schema": {} })),
                expect: Some("Pass".into()),
            }]),
        );
        let diags = check_vacuous_expected(&cfg);
        assert_eq!(diags.len(), 1, "{diags:?}");
        assert_eq!(diags[0].code, "E_CONFIG_ERROR");
        assert!(diags[0].message.contains("expect"), "{}", diags[0].message);
    }

    /// The invariant the static sweep rests on: it must reject configurations that cannot check
    /// anything, and **only** those. An assertion that constrains something and would simply not
    /// hold for a given trace is not a config defect, and reporting it here would make
    /// `assay validate` refuse working suites — the over-eager detection that earns a suppression
    /// and takes the real findings down with it.
    ///
    /// Each case below fails when evaluated against an empty episode, which is exactly the input
    /// the sweep uses internally. If a future check answered one of these from the configuration,
    /// this test fails rather than the sweep quietly growing false positives.
    #[test]
    fn does_not_flag_an_assertion_that_merely_fails_for_a_trace() {
        for assertion in [
            // Requires three calls; an empty episode has none.
            TraceAssertion::TraceMustCallTool {
                tool: "search".into(),
                min_calls: Some(3),
            },
            // Requires this order; an empty episode has no calls at all.
            TraceAssertion::TraceToolSequence {
                sequence: vec!["a".into(), "b".into()],
                allow_other_tools: true,
            },
            // Exact-sequence form, likewise unsatisfied by an empty episode.
            TraceAssertion::TraceToolSequence {
                sequence: vec!["a".into()],
                allow_other_tools: false,
            },
            // A well-formed policy the supplied arguments violate: a real failure, not a
            // configuration that checks nothing.
            TraceAssertion::ArgsValid {
                tool: "t".into(),
                test_args: Some(serde_json::json!({ "percent": 90 })),
                policy: Some(serde_json::json!({
                    "schema": { "properties": { "percent": { "type": "number", "maximum": 30 } } }
                })),
                expect: Some("pass".into()),
            },
            // A blocked call that is actually made, expected to pass: fails, and should.
            TraceAssertion::ToolBlocklist {
                test_tool_calls: Some(vec!["rm".into()]),
                policy: Some(serde_json::json!({ "blocked": ["rm"] })),
                expect: Some("pass".into()),
            },
        ] {
            let cfg = cfg_with(Expected::default(), Some(vec![assertion.clone()]));
            let diags = check_vacuous_expected(&cfg);
            assert!(
                diags.is_empty(),
                "the static sweep rejected a configuration that merely fails for a trace: \
                 {assertion:?} -> {diags:?}"
            );
        }
    }

    /// The sweep reaches the schema compiler from a caller that did not exist before, so a
    /// schema that cannot compile now has a new way to be encountered. A panic here would take
    /// down `assay validate` on the one input it most needs to survive: a config someone is
    /// trying to fix.
    ///
    /// It returns, and it says nothing. That was worth measuring rather than assuming, because
    /// an earlier version of the doc comment on `ineffective_reason` claimed the opposite — that
    /// the schema compilation the sweep performs would surface a broken schema early. It does
    /// not: the resulting diagnostic carries an evaluation-decided code, which the filter drops.
    /// Both are pinned here so the boundary is stated rather than rediscovered.
    #[test]
    fn a_schema_that_cannot_compile_neither_panics_nor_is_swept() {
        for (case, policy) in [
            (
                "type is not a schema keyword value",
                serde_json::json!({ "schema": { "type": 42 } }),
            ),
            (
                "properties is not an object",
                serde_json::json!({ "schema": { "properties": "not-an-object" } }),
            ),
            (
                "required is not an array",
                serde_json::json!({ "schema": { "required": 7 } }),
            ),
            (
                "external ref, which the hermetic compiler refuses",
                serde_json::json!({ "schema": { "$ref": "https://example.invalid/s.json" } }),
            ),
        ] {
            let cfg = cfg_with(
                Expected::default(),
                Some(vec![TraceAssertion::ArgsValid {
                    tool: "t".into(),
                    test_args: Some(serde_json::json!({ "a": 1 })),
                    policy: Some(policy),
                    expect: Some("pass".into()),
                }]),
            );
            // The assertion is that this returns rather than unwinding.
            let diags = check_vacuous_expected(&cfg);
            assert!(
                diags.is_empty(),
                "{case}: the sweep reported {diags:?}. An uncompilable schema is a broken \
                 assertion, not one that cannot fail, and widening the sweep to catch it would \
                 cost the narrowness `does_not_flag_an_assertion_that_merely_fails_for_a_trace` \
                 protects. If this is now wanted, it belongs in a schema-validity check with its \
                 own diagnostic, not in the vacuity filter."
            );
        }
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
        assert_eq!(report.diagnostics[0].code, codes::W_CFG_VACUOUS_EXPECTED);
    }
}
