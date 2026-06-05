use crate::cli::commands::evidence::cyclonedx_mlbom_model::constants::{EVENT_SOURCE, EVENT_TYPE};
use crate::cli::commands::evidence::cyclonedx_mlbom_model::reduce::reduce_model_component;
use crate::cli::commands::evidence::cyclonedx_mlbom_model::source::read_json_file;
use anyhow::{bail, Result};
use assay_evidence::types::{EvidenceEvent, ProducerMeta};
use chrono::{DateTime, Utc};
use std::path::Path;

pub(super) fn read_cyclonedx_model_event(
    input: &Path,
    bom_ref: Option<&str>,
    source_artifact_ref: &str,
    source_artifact_digest: &str,
    run_id: &str,
    import_time: DateTime<Utc>,
    producer: &ProducerMeta,
) -> Result<EvidenceEvent> {
    if run_id.contains(':') {
        bail!("run_id cannot contain ':' because event ids use run_id:seq");
    }

    let bom = read_json_file(input)?;
    let payload = reduce_model_component(
        &bom,
        bom_ref,
        source_artifact_ref,
        source_artifact_digest,
        import_time,
    )?;

    Ok(
        EvidenceEvent::new(EVENT_TYPE, EVENT_SOURCE, run_id, 0, payload)
            .with_time(import_time)
            .with_producer(producer),
    )
}
