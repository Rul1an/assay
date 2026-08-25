//! `assay project-enforcement-health` — project one existing health document.
//!
//! Guardrail: one private mapping function is the only source of observation
//! truth. The CLI reads a bounded input, dispatches on `schema`, deserializes
//! the matching typed carrier, reduces to a source status, and writes JSON.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use serde::Serialize;

use crate::cli::args::ProjectEnforcementHealthArgs;
use crate::cli::commands::monitor::monitor_next::enforcement_health::{
    EnforcementHealth, NetworkEnforcement, SCHEMA_V0,
};
use crate::enforcement_health_v1::{EnforcementHealthV1, Status as V1Status, SCHEMA_V1};
use crate::exit_codes::EXIT_CONFIG_ERROR;
use crate::output_write::write_stdout_json;

pub const SCHEMA_PROJECTION_V0: &str = "assay.enforcement_health_projection.v0";
pub const MAX_INPUT_BYTES: u64 = 65_536;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum Observation {
    Applied,
    Degraded,
    NotRequested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
struct Projection {
    schema: &'static str,
    lossy: bool,
    source_schema: &'static str,
    observation: Observation,
}

/// Status extracted from a fully typed carrier. Small enough that clippy
/// does not ask for a Box; mapping truth lives only here.
enum SourceStatus {
    V0(NetworkEnforcement),
    V1(V1Status),
}

fn project_health(status: SourceStatus) -> Option<Projection> {
    let (source_schema, observation) = match status {
        SourceStatus::V0(NetworkEnforcement::Active) => (SCHEMA_V0, Observation::Applied),
        SourceStatus::V0(NetworkEnforcement::Failed) => (SCHEMA_V0, Observation::Degraded),
        SourceStatus::V0(NetworkEnforcement::Absent) => (SCHEMA_V0, Observation::NotRequested),
        SourceStatus::V0(NetworkEnforcement::NotApplicable) => return None,
        SourceStatus::V1(V1Status::Active) => (SCHEMA_V1, Observation::Applied),
        SourceStatus::V1(V1Status::Failed) => (SCHEMA_V1, Observation::Degraded),
    };
    Some(Projection {
        schema: SCHEMA_PROJECTION_V0,
        lossy: true,
        source_schema,
        observation,
    })
}

fn read_bounded(path: &Path) -> Result<Vec<u8>, ()> {
    let file = File::open(path).map_err(|_| ())?;
    let mut buf = Vec::new();
    file.take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut buf)
        .map_err(|_| ())?;
    if buf.len() as u64 > MAX_INPUT_BYTES {
        return Err(());
    }
    Ok(buf)
}

fn parse_health(bytes: &[u8]) -> Option<SourceStatus> {
    let value: serde_json::Value = serde_json::from_slice(bytes).ok()?;
    let schema = value.get("schema")?.as_str()?;
    match schema {
        SCHEMA_V0 => {
            let health: EnforcementHealth = serde_json::from_value(value).ok()?;
            (health.schema == SCHEMA_V0).then_some(SourceStatus::V0(health.network_enforcement))
        }
        SCHEMA_V1 => {
            let health: EnforcementHealthV1 = serde_json::from_value(value).ok()?;
            (health.schema == SCHEMA_V1).then_some(SourceStatus::V1(health.status))
        }
        _ => None,
    }
}

fn fail_closed() -> anyhow::Result<i32> {
    Ok(EXIT_CONFIG_ERROR)
}

pub fn run(args: ProjectEnforcementHealthArgs) -> anyhow::Result<i32> {
    if args.format != "json" {
        return fail_closed();
    }
    let bytes = match read_bounded(&args.input) {
        Ok(bytes) => bytes,
        Err(()) => return fail_closed(),
    };
    let Some(status) = parse_health(&bytes) else {
        return fail_closed();
    };
    let Some(projection) = project_health(status) else {
        return fail_closed();
    };
    let json = serde_json::to_string(&projection)?;
    Ok(write_stdout_json(&json))
}
