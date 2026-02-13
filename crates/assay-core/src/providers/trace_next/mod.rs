//! Wave3 Step2 trace split implementation behind stable facade.

use std::collections::{HashMap, HashSet};
use std::io::BufRead;

use crate::providers::trace::TraceClient;

pub(crate) mod errors;
pub(crate) mod io;
pub(crate) mod normalize;
pub(crate) mod parse;
pub(crate) mod tests;
pub(crate) mod v2;

pub(crate) fn from_path_impl<P: AsRef<std::path::Path>>(path: P) -> anyhow::Result<TraceClient> {
    let reader = io::open_reader(path)?;

    let mut traces = HashMap::new();
    let mut request_ids = HashSet::new();
    let mut active_episodes: HashMap<String, parse::EpisodeState> = HashMap::new();

    for (i, line_res) in reader.lines().enumerate() {
        let line_no = i + 1;
        let line = line_res?;
        if line.trim().is_empty() {
            continue;
        }

        let v = parse::parse_trace_line_json(&line, line_no)?;
        let mut parsed = parse::ParsedTraceRecord::new();

        match v2::handle_typed_event(&v, &mut active_episodes, &mut parsed) {
            parse::LineDisposition::Continue => continue,
            parse::LineDisposition::MaybeInsert => {}
            parse::LineDisposition::ParseLegacy => parse::parse_legacy_record(&v, &mut parsed),
        }

        parse::insert_trace_record(&mut traces, &mut request_ids, parsed, line_no)?;
    }

    parse::flush_active_episodes(&mut traces, active_episodes);
    let fingerprint = normalize::compute_trace_fingerprint(&traces);

    Ok(TraceClient {
        traces: std::sync::Arc::new(traces),
        fingerprint,
    })
}
