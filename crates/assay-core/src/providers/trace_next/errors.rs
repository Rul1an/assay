use std::path::Path;

pub(crate) fn open_trace_file_error(path: &Path, e: &std::io::Error) -> anyhow::Error {
    anyhow::anyhow!("failed to open trace file '{}': {}", path.display(), e)
}

pub(crate) fn invalid_trace_format(
    line: &str,
    line_no: usize,
    e: &serde_json::Error,
) -> anyhow::Error {
    anyhow::anyhow!(
        "line {}: Invalid trace format. Expected JSONL object.\n  Error: {}\n  Content: {}",
        line_no,
        e,
        line.chars().take(50).collect::<String>()
    )
}

pub(crate) fn duplicate_request_id(line_no: usize, rid: &str) -> anyhow::Error {
    anyhow::anyhow!("line {}: Duplicate request_id {}", line_no, rid)
}

pub(crate) fn duplicate_prompt(prompt: &str) -> anyhow::Error {
    anyhow::anyhow!("Duplicate prompt found in trace file: {}", prompt)
}
