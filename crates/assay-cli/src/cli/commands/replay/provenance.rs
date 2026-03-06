use anyhow::Context;
use std::path::{Path, PathBuf};

pub(super) fn annotate_replay_outputs(
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<String>,
) -> anyhow::Result<()> {
    let run_json_path = PathBuf::from("run.json");
    if run_json_path.exists() {
        annotate_run_json_provenance(
            &run_json_path,
            bundle_digest,
            replay_mode,
            source_run_id.as_deref(),
        )?;
    }

    let summary_path = PathBuf::from("summary.json");
    if summary_path.exists() {
        let raw = std::fs::read(&summary_path)?;
        let summary: assay_core::report::summary::Summary = serde_json::from_slice(&raw)
            .context("failed to parse summary.json for replay provenance")?;
        let updated =
            summary.with_replay_provenance(bundle_digest.to_string(), replay_mode, source_run_id);
        assay_core::report::summary::write_summary(&updated, &summary_path)?;
    }

    Ok(())
}

pub(super) fn annotate_run_json_provenance(
    path: &Path,
    bundle_digest: &str,
    replay_mode: &str,
    source_run_id: Option<&str>,
) -> anyhow::Result<()> {
    let raw = std::fs::read(path)
        .with_context(|| format!("failed to read run json for provenance: {}", path.display()))?;
    let mut root: serde_json::Value = serde_json::from_slice(&raw).with_context(|| {
        format!(
            "failed to parse run json for provenance: {}",
            path.display()
        )
    })?;

    let Some(obj) = root.as_object_mut() else {
        anyhow::bail!("run json must be object for replay provenance");
    };

    let provenance = obj
        .entry("provenance".to_string())
        .or_insert_with(|| serde_json::json!({}));
    let Some(prov_obj) = provenance.as_object_mut() else {
        anyhow::bail!("run json provenance must be object");
    };

    prov_obj.insert("replay".to_string(), serde_json::Value::Bool(true));
    prov_obj.insert(
        "bundle_digest".to_string(),
        serde_json::Value::String(bundle_digest.to_string()),
    );
    prov_obj.insert(
        "replay_mode".to_string(),
        serde_json::Value::String(replay_mode.to_string()),
    );
    match source_run_id {
        Some(id) => {
            prov_obj.insert(
                "source_run_id".to_string(),
                serde_json::Value::String(id.to_string()),
            );
        }
        None => {
            prov_obj.insert("source_run_id".to_string(), serde_json::Value::Null);
        }
    }

    std::fs::write(path, serde_json::to_string_pretty(&root)?)?;
    Ok(())
}
