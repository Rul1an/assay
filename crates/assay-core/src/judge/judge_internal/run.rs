use crate::judge::{reliability, JudgeService};
use crate::model::TestInput;
use serde_json::json;

#[allow(clippy::too_many_arguments)]
pub(crate) async fn evaluate_impl(
    svc: &JudgeService,
    test_id: &str,
    rubric_id: &str,
    data: &TestInput,
    response_text: &str,
    suite_rubric_version: Option<&str>,
    meta: &mut serde_json::Value,
    seed: Option<u64>,
) -> anyhow::Result<()> {
    let rubric_version = suite_rubric_version.unwrap_or("v1");

    if let Some(_trace_judge) = meta.pointer(&format!("/assay/judge/{}", rubric_id)) {
        return Ok(());
    }

    if !svc.config.enabled {
        anyhow::bail!(
            "config error: test '{}' requires judge results ('{}:{}'), but judge is disabled.\n\
             hint: options:\n\
             1) run live judge: assay ci --judge openai\n\
             2) run replay/CI offline: provide trace meta at meta.assay.judge.{}\n\
             and re-run with: assay ci --trace-file traces.jsonl --no-judge",
            test_id,
            rubric_id,
            rubric_version,
            rubric_id
        );
    }

    if svc.client.is_none() {
        anyhow::bail!(
            "config error: test '{}' requires judge results ('{}:{}'), but judge client is not configured.\n\
             hint: ensure a judge client is provided when judge is enabled (e.g., configure an LLM provider or disable judge for this test).",
            test_id, rubric_id, rubric_version
        );
    }

    let should_swap_init = seed.map(|s| (s % 2) == 1).unwrap_or(false);
    let label_init = if should_swap_init {
        "Response B"
    } else {
        "Response A"
    };

    let (prompt, _) = super::prompt::build_prompt_impl(rubric_id, data, response_text, label_init);
    let input_hash = format!("{:x}", md5::compute(&prompt));
    let cache_key =
        super::cache::generate_cache_key_impl(svc, rubric_id, rubric_version, &input_hash, seed);

    if !svc.config.refresh {
        if let Some(mut cached) = svc.cache.get(&cache_key)? {
            if let Some(obj) = cached.as_object_mut() {
                obj.insert("source".to_string(), json!("cache"));
                obj.insert(
                    "cached_at".to_string(),
                    json!(chrono::Utc::now().to_rfc3339()),
                );
            }
            super::cache::inject_result_impl(svc, meta, rubric_id, cached)?;
            return Ok(());
        }
    }

    let mut votes = Vec::new();
    let mut rationales = Vec::new();
    let mut extra_calls_used = 0;

    let use_blind = svc.config.reliability.blind_labeling;
    let use_rnd = svc.config.reliability.order_randomized;
    let should_swap = use_rnd && seed.map(|s| (s % 2) == 1).unwrap_or(false);
    let candidate_label = if use_blind {
        if should_swap {
            "Response B"
        } else {
            "Response A"
        }
    } else {
        "Candidate Response"
    };

    let (prompt_text, _) =
        super::prompt::build_prompt_impl(rubric_id, data, response_text, candidate_label);
    let first_result = super::client::call_judge_impl(svc, rubric_id, &prompt_text).await?;
    votes.push(first_result.passed);
    rationales.push(first_result.rationale);

    let mut current_score = votes.iter().filter(|&&v| v).count() as f64 / votes.len() as f64;
    while svc
        .config
        .reliability
        .triggers_rerun(current_score, votes.len() as u32)
        && (votes.len() as u32) < svc.config.reliability.max_extra_calls_per_test + 1
    {
        let global_used = svc
            .global_extra_calls
            .load(std::sync::atomic::Ordering::Relaxed);
        if global_used >= svc.config.reliability.max_extra_calls_per_run {
            eprintln!(
                "[WARN] Judge soft budget exceeded: {} >= {}",
                global_used, svc.config.reliability.max_extra_calls_per_run
            );
        }

        let _iter_seed = seed.map(|s| s.wrapping_add(votes.len() as u64));
        let next_result = super::client::call_judge_impl(svc, rubric_id, &prompt_text).await?;
        votes.push(next_result.passed);
        rationales.push(next_result.rationale);
        extra_calls_used += 1;
        svc.global_extra_calls
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);

        current_score = votes.iter().filter(|&&v| v).count() as f64 / votes.len() as f64;

        let max_possible_votes = svc.config.reliability.max_extra_calls_per_test + 1;
        let passes = votes.iter().filter(|&&v| v).count();
        let fails = votes.len() - passes;
        let majority = (max_possible_votes / 2) + 1;
        if passes >= majority as usize || fails >= majority as usize {
            break;
        }
    }

    let agreement = current_score;
    let verdict = svc.config.reliability.assess(agreement);
    let passed = matches!(verdict, reliability::VerdictStatus::Pass);

    let result = json!({
        "rubric_version": rubric_version,
        "passed": passed,
        "verdict": format!("{:?}", verdict),
        "score": agreement,
        "source": "live",
        "samples": votes,
        "extra_calls_used": extra_calls_used,
        "agreement": agreement,
        "rationale": rationales.first().cloned().unwrap_or_default(),
        "judge_seed": seed,
        "swapped": should_swap,
        "cached_at": chrono::Utc::now().to_rfc3339()
    });

    svc.cache.put(
        &cache_key,
        &svc.config.provider,
        svc.config.model.as_deref().unwrap_or("default"),
        rubric_id,
        rubric_version,
        &result,
    )?;

    super::cache::inject_result_impl(svc, meta, rubric_id, result)?;
    Ok(())
}
