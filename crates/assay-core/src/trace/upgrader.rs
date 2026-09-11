use crate::trace::observation::{decode_observations, ObservedTraceEvent, TruncationObservation};
use crate::trace::schema::{
    EpisodeEnd, EpisodeStart, StepEntry, TraceEntry, TraceEntryV1, TraceEvent,
};
use crate::trace::truncation::{
    compute_sha256, compute_sha256_str, truncate_string, truncate_value_with_provenance,
};
use std::io::BufRead;

pub struct StreamUpgrader<R> {
    reader: R,
    current_line_events: std::vec::IntoIter<ObservedTraceEvent>,
}

impl<R: BufRead> StreamUpgrader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            current_line_events: vec![].into_iter(),
        }
    }

    /// Yields each event with the observations that describe it, including a new
    /// upgrader observation for EpisodeStart, Step, and ToolCall.
    pub fn observed(self) -> ObservedStream<R> {
        ObservedStream { inner: self }
    }

    fn next_observed_event(&mut self) -> Option<serde_json::Result<ObservedTraceEvent>> {
        if let Some(event) = self.current_line_events.next() {
            return Some(Ok(event));
        }

        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => return None,
            Ok(_) => {}
            Err(_) => return None,
        }

        let line = line.trim();
        if line.is_empty() {
            return self.next_observed_event();
        }

        let value: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(e) => return Some(Err(e)),
        };
        let prior = decode_observations(value.get("observations"));

        match serde_json::from_value::<TraceEntry>(value) {
            Ok(TraceEntry::V2(event)) => Some(Ok(observe_event(event, prior))),
            Ok(TraceEntry::V1(v1)) => {
                let events = upgrade_v1_to_v2(v1);
                let observed: Vec<_> = events
                    .into_iter()
                    .map(|event| observe_event(event, Vec::new()))
                    .collect();
                self.current_line_events = observed.into_iter();
                self.next_observed_event()
            }
            Err(e) => Some(Err(e)),
        }
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

fn observe_event(
    mut event: TraceEvent,
    mut prior: Vec<TruncationObservation>,
) -> ObservedTraceEvent {
    if let Some(obs) = scan_and_truncate(&mut event) {
        prior.push(obs);
    }
    ObservedTraceEvent::new(event, prior)
}

fn scan_and_truncate(event: &mut TraceEvent) -> Option<TruncationObservation> {
    match event {
        TraceEvent::EpisodeStart(e) => {
            let mut losses = truncate_value_with_provenance(&mut e.input, "input");
            losses.extend(truncate_value_with_provenance(&mut e.meta, "meta"));
            Some(TruncationObservation::upgrader(
                vec!["/input".into(), "/meta".into()],
                losses,
            ))
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
            Some(TruncationObservation::upgrader(
                vec!["/content".into(), "/meta".into()],
                losses,
            ))
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
            Some(TruncationObservation::upgrader(
                vec!["/args".into(), "/result".into()],
                losses,
            ))
        }
        TraceEvent::EpisodeEnd(_) => None,
    }
}

fn upgrade_v1_to_v2(v1: TraceEntryV1) -> Vec<TraceEvent> {
    let ts = 0;
    let episode_id = v1.request_id.clone();

    let start = TraceEvent::EpisodeStart(EpisodeStart {
        episode_id: episode_id.clone(),
        timestamp: ts,
        input: serde_json::json!({ "prompt": v1.prompt }),
        meta: v1.meta.clone(),
    });

    let step = TraceEvent::Step(StepEntry {
        episode_id: episode_id.clone(),
        step_id: format!("{}-step-0", episode_id),
        idx: 0,
        timestamp: ts + 1,
        kind: "llm_completion".to_string(),
        name: Some("model".to_string()),
        content: Some(v1.response),
        meta: serde_json::Value::Null,
        content_sha256: None,
        truncations: Vec::new(),
    });

    let end = TraceEvent::EpisodeEnd(EpisodeEnd {
        episode_id,
        timestamp: ts + 2,
        outcome: Some("pass".to_string()),
        final_output: None,
    });

    vec![start, step, end]
}
