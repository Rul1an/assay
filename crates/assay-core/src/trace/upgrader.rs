use super::migration::{convert_line, open_ids, MAX_LINE_BYTES};
use crate::trace::observation::{ObservedTraceEvent, TruncationObservation};
use crate::trace::schema::{EpisodeEnd, EpisodeStart, StepEntry, TraceEntryV1, TraceEvent};
use crate::trace::truncation::{
    compute_sha256, compute_sha256_str, truncate_string, truncate_value_with_provenance,
};
use std::io::{BufRead, Read};

pub(super) const LEGACY_STEP_SUFFIX: &str = "-step-0";

pub struct StreamUpgrader<R> {
    reader: R,
    ended: bool,
    preserve_native_digests: bool,
    line_number: u64,
    current_line_events: std::vec::IntoIter<ObservedTraceEvent>,
    // SQLite's temporary database can spill to disk for long legacy streams.
    // Exclusive iteration accesses it via get_mut, preserving Sync for Sync readers.
    legacy_ids: std::sync::Mutex<Option<rusqlite::Connection>>,
}

impl<R: BufRead> StreamUpgrader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            ended: false,
            preserve_native_digests: false,
            line_number: 0,
            current_line_events: vec![].into_iter(),
            legacy_ids: Default::default(),
        }
    }

    pub(super) fn for_migration(reader: R) -> Self {
        Self {
            preserve_native_digests: true,
            ..Self::new(reader)
        }
    }

    /// Yields each event with the observations that describe it, including a new
    /// upgrader observation for EpisodeStart, Step, and ToolCall.
    pub fn observed(self) -> ObservedStream<R> {
        ObservedStream { inner: self }
    }

    fn next_observed_event(&mut self) -> Option<serde_json::Result<ObservedTraceEvent>> {
        use serde::de::Error;
        if let Some(event) = self.current_line_events.next() {
            return Some(Ok(event));
        }
        let mut line = String::new();
        while !self.ended {
            line.clear();
            self.line_number += 1;
            match self
                .reader
                .by_ref()
                .take(MAX_LINE_BYTES + 1)
                .read_line(&mut line)
            {
                Ok(0) => break,
                Ok(n) if n as u64 > MAX_LINE_BYTES => {
                    self.ended = true;
                    return Some(Err(serde_json::Error::custom(format!(
                        "trace line {} exceeds 16 MiB",
                        self.line_number
                    ))));
                }
                Err(error) => {
                    self.ended = true;
                    return Some(Err(serde_json::Error::io(error)));
                }
                _ => {}
            }
            let source = line.trim();
            if source.is_empty() {
                continue;
            }
            let slot = self
                .legacy_ids
                .get_mut()
                .expect("legacy_ids is never locked");
            let db = match slot {
                Some(db) => db,
                None => match open_ids() {
                    Ok(db) => slot.insert(db),
                    Err(error) => return Some(Err(serde_json::Error::custom(error))),
                },
            };
            match convert_line(source, db, self.preserve_native_digests) {
                Ok(events) => {
                    self.current_line_events = events.into_iter();
                    return self.current_line_events.next().map(Ok);
                }
                Err(error) => {
                    return Some(Err(serde_json::Error::custom(format!(
                        "trace line {}: {error:#}",
                        self.line_number
                    ))))
                }
            }
        }
        self.ended = true;
        None
    }
}

impl<R: BufRead> Iterator for StreamUpgrader<R> {
    type Item = serde_json::Result<TraceEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        self.next_observed_event()
            .map(|r| r.map(ObservedTraceEvent::into_event))
    }
}

/// Additive iterator that yields [`ObservedTraceEvent`] instead of the bare event.
pub struct ObservedStream<R> {
    inner: StreamUpgrader<R>,
}

impl<R: BufRead> Iterator for ObservedStream<R> {
    type Item = serde_json::Result<ObservedTraceEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        self.inner.next_observed_event()
    }
}

pub(super) fn observe_event(
    mut event: TraceEvent,
    mut prior: Vec<TruncationObservation>,
) -> ObservedTraceEvent {
    // Foreign observations with no losses must not be carried forward across stages.
    // Otherwise, our ingest writer attaches its own digest binding to the foreign observation,
    // falsely certifying foreign completeness as origin completeness under our key.
    prior.retain(|obs| !obs.losses.is_empty());
    if let Some(obs) = scan_and_truncate(&mut event) {
        prior.push(obs);
    }
    ObservedTraceEvent::new(event, prior)
}

/// Field roots the upgrader scans for each event kind, as absolute RFC 6901 pointers.
///
/// One table for the producer (the observation's `scope`) and for consumers that
/// render a reading per declared field: a root added here is scanned and reported
/// together. `EpisodeEnd` declares nothing and gets no observation.
pub fn declared_field_roots(event: &TraceEvent) -> &'static [&'static str] {
    match event {
        TraceEvent::EpisodeStart(_) => &["/input", "/meta"],
        TraceEvent::Step(_) => &["/content", "/meta"],
        TraceEvent::ToolCall(_) => &["/args", "/result"],
        TraceEvent::EpisodeEnd(_) => &[],
    }
}

fn declared_scope(event: &TraceEvent) -> Vec<String> {
    declared_field_roots(event)
        .iter()
        .map(|root| (*root).to_string())
        .collect()
}

fn scan_and_truncate(event: &mut TraceEvent) -> Option<TruncationObservation> {
    let scope = declared_scope(event);
    match event {
        TraceEvent::EpisodeStart(e) => {
            let mut losses = truncate_value_with_provenance(&mut e.input, "input");
            losses.extend(truncate_value_with_provenance(&mut e.meta, "meta"));
            Some(TruncationObservation::upgrader(scope, losses))
        }
        TraceEvent::Step(e) => {
            let mut losses = Vec::new();
            if let Some(c) = &mut e.content {
                e.content_sha256 = Some(compute_sha256_str(c));
                if let Some(meta) = truncate_string(c, "content") {
                    losses.push(meta);
                }
            }
            losses.extend(truncate_value_with_provenance(&mut e.meta, "meta"));
            e.truncations.extend(losses.iter().cloned());
            Some(TruncationObservation::upgrader(scope, losses))
        }
        TraceEvent::ToolCall(e) => {
            e.args_sha256 = Some(compute_sha256(&e.args));
            if let Some(res) = &e.result {
                e.result_sha256 = Some(compute_sha256(res));
            }

            let mut losses = truncate_value_with_provenance(&mut e.args, "args");
            if let Some(mut result_val) = e.result.take() {
                losses.extend(truncate_value_with_provenance(&mut result_val, "result"));
                e.result = Some(result_val);
            }
            e.truncations.extend(losses.iter().cloned());
            Some(TruncationObservation::upgrader(scope, losses))
        }
        TraceEvent::EpisodeEnd(_) => None,
    }
}

pub(super) fn upgrade_v1_to_v2(
    v1: TraceEntryV1,
    model: Option<serde_json::Value>,
    untagged: bool,
) -> Vec<TraceEvent> {
    let ts = 0;
    let episode_id = v1.request_id.clone();

    let start = TraceEvent::EpisodeStart(EpisodeStart {
        episode_id: episode_id.clone(),
        timestamp: ts,
        input: serde_json::json!({ "prompt": v1.prompt }),
        meta: v1.meta.clone(),
    });

    // Typed replay interprets completion envelopes in content. Carry a bounded
    // literal envelope in the existing completion field instead, retaining its
    // original digest. Longer strings take the normal observed truncation path.
    let literal_envelope = v1.response.len() <= super::truncation::INGEST_STRING_CEILING
        && serde_json::from_str(&v1.response)
            .is_ok_and(|value| super::schema::model_completion(&value).is_some());
    let (content, content_sha256, mut step_meta) = if literal_envelope {
        let digest = compute_sha256_str(&v1.response);
        (
            None,
            Some(digest),
            serde_json::json!({ "gen_ai.completion": v1.response, (super::schema::CONTENT_IN_COMPLETION): true }),
        )
    } else {
        (Some(v1.response), None, serde_json::Value::Null)
    };
    if let Some(model @ serde_json::Value::String(_)) = model {
        step_meta["gen_ai.response.model"] = model;
    }

    let step = TraceEvent::Step(StepEntry {
        episode_id: episode_id.clone(),
        step_id: format!("{episode_id}{LEGACY_STEP_SUFFIX}"),
        idx: 0,
        timestamp: ts + 1,
        kind: if untagged {
            "legacy_replay"
        } else {
            "llm_completion"
        }
        .to_string(),
        name: (!untagged).then(|| "model".to_string()),
        content,
        meta: step_meta,
        content_sha256,
        truncations: Vec::new(),
    });

    let end = TraceEvent::EpisodeEnd(EpisodeEnd {
        episode_id,
        timestamp: ts + 2,
        outcome: None,
        final_output: None,
    });

    vec![start, step, end]
}
