//! File form shared by profile update, show, policy generation and evidence export.
use super::{json_profile, Profile, ProfileEntries, MAX_PROFILE_BYTES};
use anyhow::{anyhow, Result};
use assay_common::doc::{JsonInput, Refusal};
use assay_common::limits::{LimitKind, LimitReader};
use assay_common::unique_value::UniqueValue;
use assay_evidence::types::{
    PayloadSandboxDegraded, SandboxDegradationComponent, SandboxDegradationMode,
    SandboxDegradationReasonCode,
};
use std::{fs::File, io::Read, path::Path};

assay_common::closed! {
    root struct ProfileDocument {
        version: String,
        name: String,
        created_at: String,
        updated_at: String,
        scope: Option<String>,
        total_runs: u32,
        #[default] run_ids: Vec<String>,
        #[default] run_id_digests: Vec<String>,
        entries: ProfileEntries,
        #[default] sandbox_degradations: Vec<Degradation>,
    }
}

assay_common::closed! {
    struct Degradation {
        reason_code: SandboxDegradationReasonCode,
        degradation_mode: SandboxDegradationMode,
        component: SandboxDegradationComponent,
        detail: Option<String>,
    }
}

pub fn load_profile(path: &Path) -> Result<Profile> {
    let mut content = String::new();
    LimitReader::new(
        File::open(path)?,
        MAX_PROFILE_BYTES as u64,
        LimitKind::SourceBytes,
    )
    .read_to_string(&mut content)?;
    let ProfileDocument {
        version,
        name,
        created_at,
        updated_at,
        scope,
        total_runs,
        run_ids,
        run_id_digests,
        entries,
        sandbox_degradations,
    } = if json_profile(path) {
        let UniqueValue(source) = serde_json::from_str(&content)?;
        JsonInput::read_value(&source)
    } else {
        assay_common::doc::load_with_string_keys(&content)
    }
    .map_err(|e| match e {
        Refusal::UnknownKeys(keys) => anyhow!("unknown profile keys: {keys:?}"),
        Refusal::NotAMapping { path, expected } => {
            anyhow!("expected a {expected} mapping at {path:?}")
        }
        Refusal::Syntax { message, .. } | Refusal::Shape { message, .. } => anyhow!("{message}"),
        _ => anyhow!("invalid profile document"),
    })?;
    Ok(Profile {
        version,
        name,
        created_at,
        updated_at,
        scope,
        total_runs,
        run_ids: run_ids.into(),
        run_id_digests: run_id_digests.into(),
        entries,
        sandbox_degradations: sandbox_degradations
            .into_iter()
            .map(
                |Degradation {
                     reason_code,
                     degradation_mode,
                     component,
                     detail,
                 }| PayloadSandboxDegraded {
                    reason_code,
                    degradation_mode,
                    component,
                    detail,
                },
            )
            .collect(),
    })
}
