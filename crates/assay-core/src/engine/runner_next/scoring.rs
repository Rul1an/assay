use super::super::Runner;
use super::cache as cache_next;
use crate::model::{EvalConfig, LlmResponse, TestCase};

pub(crate) async fn enrich_semantic_impl(
    runner: &Runner,
    _cfg: &EvalConfig,
    tc: &TestCase,
    resp: &mut LlmResponse,
) -> anyhow::Result<()> {
    use crate::model::Expected;

    let Expected::SemanticSimilarityTo {
        semantic_similarity_to,
        ..
    } = &tc.expected
    else {
        return Ok(());
    };

    if resp.meta.pointer("/assay/embeddings/response").is_some()
        && resp.meta.pointer("/assay/embeddings/reference").is_some()
    {
        return Ok(());
    }

    if runner.policy.replay_strict {
        anyhow::bail!("config error: --replay-strict is on, but embeddings are missing in trace. Run 'assay trace precompute-embeddings' or disable strict mode.");
    }

    let embedder = runner.embedder.as_ref().ok_or_else(|| {
        anyhow::anyhow!(
            "config error: semantic_similarity_to requires an embedder (--embedder) or trace meta embeddings"
        )
    })?;

    let model_id = embedder.model_id();

    let (resp_vec, src_resp) =
        cache_next::embed_text_impl(runner, &model_id, embedder.as_ref(), &resp.text).await?;
    let (ref_vec, src_ref) =
        cache_next::embed_text_impl(runner, &model_id, embedder.as_ref(), semantic_similarity_to)
            .await?;

    if !resp.meta.get("assay").is_some_and(|v| v.is_object()) {
        resp.meta["assay"] = serde_json::json!({});
    }
    resp.meta["assay"]["embeddings"] = serde_json::json!({
        "model": model_id,
        "response": resp_vec,
        "reference": ref_vec,
        "source_response": src_resp,
        "source_reference": src_ref
    });

    Ok(())
}

pub(crate) async fn enrich_judge_impl(
    runner: &Runner,
    cfg: &EvalConfig,
    tc: &TestCase,
    resp: &mut LlmResponse,
) -> anyhow::Result<()> {
    use crate::model::Expected;

    let (rubric_id, rubric_version) = match &tc.expected {
        Expected::Faithfulness { rubric_version, .. } => {
            ("faithfulness", rubric_version.as_deref())
        }
        Expected::Relevance { rubric_version, .. } => ("relevance", rubric_version.as_deref()),
        _ => return Ok(()),
    };

    let has_trace = resp
        .meta
        .pointer(&format!("/assay/judge/{}", rubric_id))
        .is_some();
    if runner.policy.replay_strict && !has_trace {
        anyhow::bail!("config error: --replay-strict is on, but judge results are missing in trace for '{}'. Run 'assay trace precompute-judge' or disable strict mode.", rubric_id);
    }

    let judge = runner.judge.as_ref().ok_or_else(|| {
        anyhow::anyhow!("config error: judge required but service not initialized")
    })?;

    let test_seed = cfg.settings.seed.map(|s: u64| {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        s.hash(&mut hasher);
        tc.id.hash(&mut hasher);
        hasher.finish()
    });

    judge
        .evaluate(
            &tc.id,
            rubric_id,
            &tc.input,
            &resp.text,
            rubric_version,
            &mut resp.meta,
            test_seed,
        )
        .await?;

    Ok(())
}
