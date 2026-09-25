use super::super::TraceLoadError;
use std::path::Path;

pub(crate) fn open_trace_file_error(path: &Path, e: std::io::Error) -> TraceLoadError {
    TraceLoadError::Open {
        path: path.display().to_string(),
        source: e,
    }
}

pub(crate) fn invalid_trace_format(
    line: &str,
    line_no: usize,
    e: &serde_json::Error,
) -> TraceLoadError {
    TraceLoadError::InvalidLine {
        line_no,
        error: e.to_string(),
        snippet: line.chars().take(50).collect(),
    }
}

pub(crate) fn duplicate_request_id(line_no: usize, rid: &str) -> TraceLoadError {
    TraceLoadError::DuplicateRequestId {
        line_no,
        request_id: rid.to_string(),
    }
}

pub(crate) fn duplicate_prompt(prompt: &str) -> TraceLoadError {
    TraceLoadError::DuplicatePrompt {
        prompt: prompt.to_string(),
    }
}

pub(crate) fn unreadable_trace_line(
    path: &Path,
    line_no: usize,
    source: std::io::Error,
) -> TraceLoadError {
    TraceLoadError::UnreadableLine {
        path: path.display().to_string(),
        line_no,
        source,
    }
}
