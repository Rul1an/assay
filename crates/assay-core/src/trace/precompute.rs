use super::schema::TraceEntryV1;
use crate::judge::JudgeService;
use crate::model::EvalConfig;
use crate::providers::embedder::Embedder;
use anyhow::Context;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::sync::Arc;

pub async fn precompute_embeddings(
    input: &Path,
    output: &Path,
    embedder: Arc<dyn Embedder>,
    model: &str,
    _config: &EvalConfig, // potentially unused if we trust trace
) -> anyhow::Result<()> {
    let file = File::open(input).context("failed to open input trace file")?;
    let reader = BufReader::new(file);
    let mut out_file = File::create(output).context("failed to create output trace file")?;

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue; // Skip empty lines
        }

        let mut entry: TraceEntryV1 = serde_json::from_str(&line)
            .context(format!("failed to parse trace entry at line {}", i + 1))?;

        // Check if already embedded
        let has_embedding = entry.meta.pointer("/assay/embeddings/response").is_some()
            && entry.meta.pointer("/assay/embeddings/reference").is_some();

        if !has_embedding {
            eprintln!("Embedding entry {}...", entry.request_id);

            // Heuristic: We need a "reference" text to embed against?
            // But traces don't inherently have "expected" values unless we join with Config?
            // Ah, for "semantic similarity", the expected value is in the Config (TestCase), not the trace.
            // BUT, the runner logic expects `meta.assay.embeddings.reference`.
            // If we precompute, we must know the TestCase expected value.
            // Implication: We MUST verify the trace against the config and find the matching TestCase.

            // For MVP, if we can't find the test case, we skip precompute for that entry?
            // Or we just embed the RESPONSE (which is always there).
            // Let's see `runner.rs:enrich_semantic`: it embeds `resp.text` AND `expected.semantic_similarity_to`.

            // This implies `precompute_embeddings` needs to look up the `TestCase` by `entry.request_id == tc.id`.
            // If we find it, and it expects semantic similarity, we embed BOTH.

            // Find test case
            let matching_tc = _config.tests.iter().find(|tc| tc.id == entry.request_id);

            if let Some(tc) = matching_tc {
                use crate::model::Expected;
                if let Expected::SemanticSimilarityTo {
                    semantic_similarity_to,
                    ..
                } = &tc.expected
                {
                    let resp_vec = embedder.embed(&entry.response).await?;
                    let ref_vec = embedder.embed(semantic_similarity_to).await?;

                    // Patch meta
                    if !entry.meta.is_object() {
                        entry.meta = serde_json::json!({});
                    }
                    if !entry.meta.get("assay").is_some_and(|v| v.is_object()) {
                        entry.meta["assay"] = serde_json::json!({});
                    }

                    entry.meta["assay"]["embeddings"] = serde_json::json!({
                        "model": model,
                        "response": resp_vec,
                        "reference": ref_vec,
                        "source_response": "precomputed",
                        "source_reference": "precomputed"
                    });
                }
                // If not semantic similarity, do we need embeddings? Maybe for classifiers? Not yet.
            }
        }

        let out_line = serde_json::to_string(&entry)?;
        writeln!(out_file, "{}", out_line)?;
    }

    Ok(())
}

pub async fn precompute_judge(
    input: &Path,
    output: &Path,
    judge: &JudgeService,
    config: &EvalConfig,
) -> anyhow::Result<()> {
    let file = File::open(input).context("failed to open input trace file")?;
    let reader = BufReader::new(file);
    let mut out_file = File::create(output).context("failed to create output trace file")?;

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let mut entry: TraceEntryV1 = serde_json::from_str(&line)
            .context(format!("failed to parse trace entry at line {}", i + 1))?;

        // Check matching TestCase
        let matching_tc = config.tests.iter().find(|tc| tc.id == entry.request_id);

        if let Some(tc) = matching_tc {
            use crate::model::Expected;
            let (rubric_id, rubric_version) = match &tc.expected {
                Expected::Faithfulness { rubric_version, .. } => {
                    ("faithfulness", rubric_version.as_deref())
                }
                Expected::Relevance { rubric_version, .. } => {
                    ("relevance", rubric_version.as_deref())
                }
                Expected::JudgeCriteria { .. } => ("custom", None),
                _ => ("none", None),
            };

            if rubric_id != "none" {
                // Check if already judged
                let existing = entry
                    .meta
                    .pointer(&format!("/assay/judge/{}", rubric_id))
                    .is_some();

                if !existing {
                    eprintln!("Judging entry {} ({}) ...", entry.request_id, rubric_id);

                    use crate::model::TestInput;
                    let input = TestInput {
                        prompt: entry.prompt.clone(),
                        context: None, // Traces usually contain full context in prompt or meta? For V1, flat prompt.
                    };

                    // We modify entry.meta in place
                    // But JudgeService::evaluate expects `&mut serde_json::Value` (meta)
                    // and typically writes to `assay.judge.{rubric_id}`.
                    judge
                        .evaluate(
                            &entry.request_id,
                            rubric_id,
                            &input,
                            &entry.response,
                            rubric_version,
                            &mut entry.meta,
                        )
                        .await?;
                }
            }
        }

        let out_line = serde_json::to_string(&entry)?;
        writeln!(out_file, "{}", out_line)?;
    }

    Ok(())
}
