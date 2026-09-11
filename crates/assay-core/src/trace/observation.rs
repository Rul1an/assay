//! Truncation observations that ride next to trace rows on 6.x (ADR-050).
//!
//! No field is added to `EpisodeStart`, `StepEntry`, or `ToolCallEntry`. A present
//! observation with empty `losses` is measured-clean; an absent or empty list is
//! unmeasured. Reported loss dominates.

use super::schema::{TraceEvent, TruncationMeta};
use super::truncation::compute_sha256;
use serde::{Serialize, Serializer};
use serde_json::Value;

/// Wire version this reader understands. Higher `v` values are treated as absent.
pub const OBSERVATION_VERSION: u32 = 1;

/// Stage name written by [`super::upgrader::StreamUpgrader`].
pub const UPGRADER_STAGE: &str = "assay.trace.upgrader";

/// One stage's report on the bytes it emitted.
#[derive(Debug, Clone, Serialize, serde::Deserialize, PartialEq)]
#[non_exhaustive]
pub struct TruncationObservation {
    pub v: u32,
    pub stage: String,
    pub ceiling: usize,
    pub scope: Vec<String>,
    pub losses: Vec<TruncationMeta>,
}

impl TruncationObservation {
    pub fn new(
        stage: impl Into<String>,
        ceiling: usize,
        scope: Vec<String>,
        losses: Vec<TruncationMeta>,
    ) -> Self {
        Self {
            v: OBSERVATION_VERSION,
            stage: stage.into(),
            ceiling,
            scope,
            losses,
        }
    }

    pub fn upgrader(scope: Vec<String>, losses: Vec<TruncationMeta>) -> Self {
        Self::new(
            UPGRADER_STAGE,
            super::truncation::INGEST_STRING_CEILING,
            scope,
            losses,
        )
    }
}

/// A [`TraceEvent`] plus the observations that sat on its JSONL line.
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct ObservedTraceEvent {
    event: TraceEvent,
    observations: Vec<TruncationObservation>,
}

impl ObservedTraceEvent {
    pub fn new(event: TraceEvent, observations: Vec<TruncationObservation>) -> Self {
        Self {
            event,
            observations,
        }
    }

    pub fn event(&self) -> &TraceEvent {
        &self.event
    }

    pub fn observations(&self) -> &[TruncationObservation] {
        &self.observations
    }

    pub fn into_event(self) -> TraceEvent {
        self.event
    }

    pub fn reported_truncations(&self) -> &[TruncationMeta] {
        match &self.event {
            TraceEvent::Step(s) => &s.truncations,
            TraceEvent::ToolCall(t) => &t.truncations,
            _ => &[],
        }
    }

    /// Steps and tool calls carry `truncations`; EpisodeStart does not, so parity is vacuous.
    pub fn requires_loss_parity(&self) -> bool {
        matches!(self.event, TraceEvent::Step(_) | TraceEvent::ToolCall(_))
    }
}

impl Serialize for ObservedTraceEvent {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut value = serde_json::to_value(&self.event).map_err(serde::ser::Error::custom)?;
        if !self.observations.is_empty() {
            let obs =
                serde_json::to_value(&self.observations).map_err(serde::ser::Error::custom)?;
            value
                .as_object_mut()
                .ok_or_else(|| serde::ser::Error::custom("event must serialize as an object"))?
                .insert("observations".into(), obs);
        }
        value.serialize(serializer)
    }
}

/// A field reading. Exhaustive: a new reading must force consumers to decide.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TruncationReading {
    Lossy,
    MeasuredClean { stage: String, ceiling: usize },
    Unmeasured,
}

/// Parse a JSONL object, treating unknown versions and malformed `observations` as absent.
pub fn parse_observed_event(value: &Value) -> serde_json::Result<ObservedTraceEvent> {
    let observations = decode_observations(value.get("observations"));
    let event = serde_json::from_value(value.clone())?;
    Ok(ObservedTraceEvent::new(event, observations))
}

pub fn parse_observed_line(line: &str) -> serde_json::Result<ObservedTraceEvent> {
    let value: Value = serde_json::from_str(line)?;
    parse_observed_event(&value)
}

pub(crate) fn decode_observations(raw: Option<&Value>) -> Vec<TruncationObservation> {
    let Some(value) = raw else {
        return Vec::new();
    };
    let Some(arr) = value.as_array() else {
        return Vec::new();
    };
    arr.iter().filter_map(decode_one_observation).collect()
}

fn decode_one_observation(value: &Value) -> Option<TruncationObservation> {
    let obs: TruncationObservation = serde_json::from_value(value.clone()).ok()?;
    if obs.v != OBSERVATION_VERSION {
        return None;
    }
    Some(obs)
}

/// SQLite target key for a tool call: JSON `[step_id, call_index]`.
pub fn tool_call_target_key(step_id: &str, call_index: u32) -> String {
    serde_json::to_string(&(step_id, call_index)).unwrap_or_else(|_| "[]".to_string())
}

/// SHA-256 over the JSON array of bound column values as stored.
///
/// One function at write and at read. A mismatch makes the observation absent, losses included.
pub fn bound_sha256(column_values: &[Option<&str>]) -> String {
    let arr = Value::Array(
        column_values
            .iter()
            .map(|v| match v {
                Some(s) => Value::String((*s).to_owned()),
                None => Value::Null,
            })
            .collect(),
    );
    compute_sha256(&arr)
}

pub fn episode_column_values(e: &super::schema::EpisodeStart) -> (String, String) {
    let prompt_val = e.input.get("prompt").unwrap_or(&Value::Null);
    let prompt_str = if let Some(s) = prompt_val.as_str() {
        s.to_string()
    } else {
        serde_json::to_string(prompt_val).unwrap_or_default()
    };
    let meta = serde_json::to_string(&e.meta).unwrap_or_default();
    (prompt_str, meta)
}

pub fn step_column_values(e: &super::schema::StepEntry) -> (Option<String>, String) {
    let meta = serde_json::to_string(&e.meta).unwrap_or_default();
    (e.content.clone(), meta)
}

pub fn tool_call_column_values(e: &super::schema::ToolCallEntry) -> (String, Option<String>) {
    let args = serde_json::to_string(&e.args).unwrap_or_default();
    let result = e
        .result
        .as_ref()
        .map(|r| serde_json::to_string(r).unwrap_or_default());
    (args, result)
}

/// One function for JSONL and SQLite. The SQLite reader applies the binding check first.
pub fn read_truncation(
    pointer: &str,
    truncations: &[TruncationMeta],
    observations: &[TruncationObservation],
    trusted_stages: &[&str],
    require_parity: bool,
) -> TruncationReading {
    if reported_loss_at_or_under(pointer, truncations, observations) {
        return TruncationReading::Lossy;
    }
    for obs in observations {
        if !observation_supports_clean(obs, truncations, trusted_stages, require_parity) {
            continue;
        }
        if !scope_covers_field(&obs.scope, pointer) {
            continue;
        }
        if obs
            .losses
            .iter()
            .any(|loss| pointer_covers(pointer, &loss.field))
        {
            continue;
        }
        return TruncationReading::MeasuredClean {
            stage: obs.stage.clone(),
            ceiling: obs.ceiling,
        };
    }
    TruncationReading::Unmeasured
}

pub fn read_observed(
    observed: &ObservedTraceEvent,
    pointer: &str,
    trusted_stages: &[&str],
) -> TruncationReading {
    read_truncation(
        pointer,
        observed.reported_truncations(),
        observed.observations(),
        trusted_stages,
        observed.requires_loss_parity(),
    )
}

fn is_known_version(obs: &TruncationObservation) -> bool {
    obs.v == OBSERVATION_VERSION
}

fn reported_loss_at_or_under(
    pointer: &str,
    truncations: &[TruncationMeta],
    observations: &[TruncationObservation],
) -> bool {
    truncations
        .iter()
        .any(|loss| pointer_covers(pointer, &loss.field))
        || observations
            .iter()
            .filter(|obs| is_known_version(obs))
            .any(|obs| {
                obs.losses
                    .iter()
                    .any(|loss| pointer_covers(pointer, &loss.field))
            })
}

fn observation_supports_clean(
    obs: &TruncationObservation,
    truncations: &[TruncationMeta],
    trusted_stages: &[&str],
    require_parity: bool,
) -> bool {
    if !is_known_version(obs) {
        return false;
    }
    if obs.stage.is_empty() {
        return false;
    }
    if !trusted_stages.iter().any(|stage| *stage == obs.stage) {
        return false;
    }
    if require_parity && !parity_holds(obs, truncations) {
        return false;
    }
    true
}

fn parity_holds(obs: &TruncationObservation, truncations: &[TruncationMeta]) -> bool {
    obs.losses
        .iter()
        .all(|loss| truncations.iter().any(|t| t == loss))
}

/// Scope covers whole RFC 6901 segments only: `/meta` covers `/meta/a`, not `/meta2`.
fn scope_covers_field(scope: &[String], pointer: &str) -> bool {
    scope.iter().any(|s| pointer_covers(s, pointer))
}

/// `parent` covers `child` when they are equal or `child` is a descendant segment.
fn pointer_covers(parent: &str, child: &str) -> bool {
    child == parent
        || (child.len() > parent.len()
            && child.as_bytes().get(parent.len()) == Some(&b'/')
            && child.starts_with(parent))
}
