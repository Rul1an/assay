use super::schema::TraceEntryV1;
use anyhow::Context;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;

pub fn ingest_file(input: &Path, output: &Path) -> anyhow::Result<()> {
    let file = File::open(input).context("failed to open input file")?;
    let reader = BufReader::new(file);

    let mut out_file = File::create(output).context("failed to create output file")?;

    for (i, line) in reader.lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }

        let raw: serde_json::Value = serde_json::from_str(&line)
            .context(format!("failed to parse JSON at line {}", i + 1))?;

        // Heuristic mapping or assume generic structure
        // If already V1, pass through. Else map.
        // For MVP, we'll implement a flexible "best effort" mapping.

        let entry = if let Ok(v1) = serde_json::from_value::<TraceEntryV1>(raw.clone()) {
            v1
        } else {
            // Try to map from common formats or just raw
            let req_id = raw
                .get("request_id")
                .or_else(|| raw.get("id"))
                .and_then(|v| v.as_str())
                .unwrap_or_else(|| "unknown");

            let prompt = raw
                .get("prompt")
                .or_else(|| raw.get("input"))
                .and_then(|v| v.as_str())
                .unwrap_or_default(); // Or fail? For now default strict.

            let response = raw
                .get("response")
                .or_else(|| raw.get("output"))
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            TraceEntryV1 {
                schema_version: 1,
                entry_type: "verdict.trace".into(),
                request_id: format!("{}_{}", req_id, i), // Ensure uniqueness if needed
                prompt: prompt.to_string(),
                response: response.to_string(),
                meta: serde_json::json!({
                    "original": raw
                }),
            }
        };

        let out_line = serde_json::to_string(&entry)?;
        writeln!(out_file, "{}", out_line)?;
    }

    Ok(())
}
