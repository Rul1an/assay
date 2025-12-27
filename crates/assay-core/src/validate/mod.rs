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
    // Could add summary stats here later
}

pub async fn validate(
    cfg: &EvalConfig,
    opts: &ValidateOptions,
    _resolver: &PathResolver,
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

    // Return early if basic files missing to avoid noise
    if !diags.is_empty() {
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
            // Actually TraceClient doesn't expose keys publically yet.
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
        Expected::Faithfulness { .. } => {
            if resp.meta.pointer("/assay/judge/faithfulness").is_none() {
                missing.push(serde_json::json!({
                    "requirement": "judge_faithfulness",
                    "needed_by": ["faithfulness"],
                    "meta_path": "meta.assay.judge.faithfulness"
                }));
            }
        }
        Expected::Relevance { .. } => {
            if resp.meta.pointer("/assay/judge/relevance").is_none() {
                missing.push(serde_json::json!({
                    "requirement": "judge_relevance",
                    "needed_by": ["relevance"],
                    "meta_path": "meta.assay.judge.relevance"
                }));
            }
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
