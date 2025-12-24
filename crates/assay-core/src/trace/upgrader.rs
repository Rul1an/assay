use crate::trace::schema::{
    EpisodeEnd, EpisodeStart, StepEntry, TraceEntry, TraceEntryV1, TraceEvent,
};
use std::io::BufRead;

pub struct StreamUpgrader<R> {
    reader: R,
    current_line_events: std::vec::IntoIter<TraceEvent>,
}

impl<R: BufRead> StreamUpgrader<R> {
    pub fn new(reader: R) -> Self {
        Self {
            reader,
            current_line_events: vec![].into_iter(),
        }
    }
}

impl<R: BufRead> Iterator for StreamUpgrader<R> {
    type Item = serde_json::Result<TraceEvent>;

    fn next(&mut self) -> Option<Self::Item> {
        // If we have buffered events from a V1 upgrade, verify/return them
        if let Some(event) = self.current_line_events.next() {
            return Some(Ok(event));
        }

        // Read next line
        let mut line = String::new();
        match self.reader.read_line(&mut line) {
            Ok(0) => return None, // EOF
            Ok(_) => {}
            Err(_) => return None, // Or handle error? Iterator usually expects Option<T>
        }

        let line = line.trim();
        if line.is_empty() {
            return self.next();
        }

        match serde_json::from_str::<TraceEntry>(line) {
            Ok(TraceEntry::V2(mut event)) => {
                apply_truncation(&mut event);
                Some(Ok(event))
            }
            Ok(TraceEntry::V1(v1)) => {
                let mut events = upgrade_v1_to_v2(v1);
                for e in &mut events {
                    apply_truncation(e);
                }
                self.current_line_events = events.into_iter();
                self.next()
            }
            Err(e) => Some(Err(e)),
        }
    }
}

fn apply_truncation(event: &mut TraceEvent) {
    use super::truncation::{
        compute_sha256, compute_sha256_str, truncate_string, truncate_value_with_provenance,
    };
    match event {
        TraceEvent::EpisodeStart(e) => {
            truncate_value_with_provenance(&mut e.input, "input");
            truncate_value_with_provenance(&mut e.meta, "meta");
        }
        TraceEvent::Step(e) => {
            if let Some(c) = &mut e.content {
                // Compute hash before truncation
                e.content_sha256 = Some(compute_sha256_str(c));
                if let Some(meta) = truncate_string(c, "content") {
                    e.truncations.push(meta);
                }
            }
            e.truncations
                .extend(truncate_value_with_provenance(&mut e.meta, "meta"));
        }
        TraceEvent::ToolCall(e) => {
            // Compute hashes
            e.args_sha256 = Some(compute_sha256(&e.args));
            if let Some(res) = &e.result {
                e.result_sha256 = Some(compute_sha256(res));
            }

            e.truncations
                .extend(truncate_value_with_provenance(&mut e.args, "args"));

            if let Some(mut result_val) = e.result.take() {
                e.truncations
                    .extend(truncate_value_with_provenance(&mut result_val, "result"));
                e.result = Some(result_val);
            }
        }
        TraceEvent::EpisodeEnd(_) => {}
    }
}

fn upgrade_v1_to_v2(v1: TraceEntryV1) -> Vec<TraceEvent> {
    let ts = 0;
    // Ideally extract from meta if possible, but keep deterministic.

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
        content_sha256: None, // Filled later
        truncations: Vec::new(),
    });

    let end = TraceEvent::EpisodeEnd(EpisodeEnd {
        episode_id,
        timestamp: ts + 2,
        outcome: Some("pass".to_string()), // V1 usually implies successful run?
        final_output: None,
    });

    vec![start, step, end]
}
