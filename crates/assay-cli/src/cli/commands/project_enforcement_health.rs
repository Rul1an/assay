//! `assay project-enforcement-health` — project one existing health document.
//!
//! Guardrail: one private mapping function is the only source of observation
//! truth. The CLI reads a bounded input, dispatches on `schema`, deserializes
//! the matching typed carrier, checks producer-legal `active` before reducing
//! to a source status, and writes JSON.

use std::fs::File;
use std::io::Read;
use std::path::Path;

use assay_evidence::types::PayloadSandboxDegraded;
use assay_evidence::{BundleReader, VerifyLimits};
use serde::Serialize;

use crate::cli::args::ProjectEnforcementHealthArgs;
use crate::cli::commands::monitor::monitor_next::enforcement_health::{
    EnforcementClass as V0Class, EnforcementHealth, NetworkEnforcement, SCHEMA_V0,
};
use crate::enforcement_health_v1::{
    EnforcementClass as V1Class, EnforcementHealthV1, Status as V1Status, SCHEMA_V1,
};
use crate::exit_codes::EXIT_CONFIG_ERROR;
use crate::output_write::write_stdout_json;

pub const SCHEMA_PROJECTION_V0: &str = "assay.enforcement_health_projection.v0";
pub const MAX_INPUT_BYTES: u64 = 65_536;
const SCHEMA_SANDBOX_DEGRADED: &str = "assay.sandbox.degraded";

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
    SandboxDegraded,
}

fn project_health(status: SourceStatus) -> Option<Projection> {
    let (source_schema, observation) = match status {
        SourceStatus::V0(NetworkEnforcement::Active) => (SCHEMA_V0, Observation::Applied),
        SourceStatus::V0(NetworkEnforcement::Failed) => (SCHEMA_V0, Observation::Degraded),
        SourceStatus::V0(NetworkEnforcement::Absent) => (SCHEMA_V0, Observation::NotRequested),
        SourceStatus::V0(NetworkEnforcement::NotApplicable) => return None,
        SourceStatus::V1(V1Status::Active) => (SCHEMA_V1, Observation::Applied),
        SourceStatus::V1(V1Status::Failed) => (SCHEMA_V1, Observation::Degraded),
        SourceStatus::SandboxDegraded => (SCHEMA_SANDBOX_DEGRADED, Observation::Degraded),
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

/// `active` is applied only when the producer invariants still hold.
/// Failed/absent stay status-only; this slice does not grow a failed framework.
fn v0_active_is_producer_legal(health: &EnforcementHealth) -> bool {
    health.attach_confirmed && health.enforcement_class == V0Class::Strong
}

fn v1_active_is_producer_legal(health: &EnforcementHealthV1) -> bool {
    health.enforcement_class == V1Class::Strong
        && health.failure.is_none()
        && health.landlock.no_new_privs_confirmed
        && health.landlock.restrict_self_confirmed
}

fn parse_health_value(value: serde_json::Value) -> Option<SourceStatus> {
    let schema = value.get("schema")?.as_str()?;
    match schema {
        SCHEMA_V0 => {
            let health: EnforcementHealth = serde_json::from_value(value).ok()?;
            if health.schema != SCHEMA_V0 {
                return None;
            }
            if health.network_enforcement == NetworkEnforcement::Active
                && !v0_active_is_producer_legal(&health)
            {
                return None;
            }
            Some(SourceStatus::V0(health.network_enforcement))
        }
        SCHEMA_V1 => {
            let health: EnforcementHealthV1 = serde_json::from_value(value).ok()?;
            if health.schema != SCHEMA_V1 {
                return None;
            }
            if health.status == V1Status::Active && !v1_active_is_producer_legal(&health) {
                return None;
            }
            Some(SourceStatus::V1(health.status))
        }
        _ => None,
    }
}

fn parse_health(bytes: &[u8]) -> Option<SourceStatus> {
    parse_health_value(serde_json::from_slice(bytes).ok()?)
}

fn projection_bundle_limits() -> VerifyLimits {
    VerifyLimits {
        max_bundle_bytes: MAX_INPUT_BYTES,
        max_decode_bytes: 1024 * 1024,
        max_manifest_bytes: MAX_INPUT_BYTES,
        max_events_bytes: 512 * 1024,
        max_events: 1_000,
        max_line_bytes: MAX_INPUT_BYTES as usize,
        max_path_len: 256,
        max_json_depth: 64,
    }
}

fn input_is_gzip(path: &Path) -> Result<bool, ()> {
    let mut file = File::open(path).map_err(|_| ())?;
    let mut magic = [0_u8; 2];
    let read = file.read(&mut magic).map_err(|_| ())?;
    Ok(read == magic.len() && magic == [0x1f, 0x8b])
}

fn parse_verified_bundle(path: &Path) -> Option<SourceStatus> {
    let file = File::open(path).ok()?;
    let reader = BundleReader::open_with_limits(file, projection_bundle_limits()).ok()?;
    let mut degradation_seen = false;
    let mut active_health_seen = false;

    for event in reader.events() {
        let event = event.ok()?;
        match event.type_.as_str() {
            SCHEMA_SANDBOX_DEGRADED => {
                if degradation_seen {
                    return None;
                }
                serde_json::from_value::<PayloadSandboxDegraded>(event.payload).ok()?;
                degradation_seen = true;
            }
            SCHEMA_V0 | SCHEMA_V1 => {
                let status = parse_health_value(event.payload)?;
                active_health_seen |= matches!(
                    status,
                    SourceStatus::V0(NetworkEnforcement::Active)
                        | SourceStatus::V1(V1Status::Active)
                );
            }
            _ => {}
        }
    }

    if !degradation_seen || active_health_seen {
        return None;
    }
    Some(SourceStatus::SandboxDegraded)
}

fn fail_closed() -> anyhow::Result<i32> {
    Ok(EXIT_CONFIG_ERROR)
}

pub fn run(args: ProjectEnforcementHealthArgs) -> anyhow::Result<i32> {
    if args.format != "json" {
        return fail_closed();
    }
    let status = match input_is_gzip(&args.input) {
        Ok(true) => parse_verified_bundle(&args.input),
        Ok(false) => read_bounded(&args.input)
            .ok()
            .and_then(|bytes| parse_health(&bytes)),
        Err(()) => None,
    };
    let Some(status) = status else {
        return fail_closed();
    };
    let Some(projection) = project_health(status) else {
        return fail_closed();
    };
    let json = serde_json::to_string(&projection)?;
    Ok(write_stdout_json(&json))
}
