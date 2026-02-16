use crate::judge::JudgeService;
use serde_json::json;

pub(crate) fn generate_cache_key_impl(
    svc: &JudgeService,
    rubric_id: &str,
    rubric_version: &str,
    input_hash: &str,
    seed: Option<u64>,
) -> String {
    let raw = format!(
        "{}:{}:{}:{}:{}:{}:{}:{}:{}:{}:{:?}",
        svc.config.provider,
        svc.config.model.as_deref().unwrap_or(""),
        rubric_id,
        rubric_version,
        svc.config.temperature,
        svc.config.max_tokens,
        svc.config.samples,
        svc.config.system_prompt_version,
        input_hash,
        serde_json::to_string(&svc.config.reliability).unwrap_or_else(|_| "err".to_string()),
        seed
    );
    format!("{:x}", md5::compute(raw))
}

pub(crate) fn inject_result_impl(
    _svc: &JudgeService,
    meta: &mut serde_json::Value,
    rubric_id: &str,
    result: serde_json::Value,
) -> anyhow::Result<()> {
    if let Some(obj) = meta.as_object_mut() {
        let assay = obj
            .entry("assay")
            .or_insert(json!({}))
            .as_object_mut()
            .unwrap();
        let judge = assay
            .entry("judge")
            .or_insert(json!({}))
            .as_object_mut()
            .unwrap();
        judge.insert(rubric_id.to_string(), result);
    }
    Ok(())
}
