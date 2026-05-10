use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;

pub(super) fn parse_input_documents(input_text: &str) -> Result<Vec<Value>> {
    let trimmed = input_text.trim();
    if trimmed.is_empty() {
        bail!("input contains no JSON documents");
    }

    if let Ok(value) = serde_json::from_str::<Value>(trimmed) {
        match value {
            Value::Object(_) => return Ok(vec![value]),
            Value::Array(values) => {
                if values.is_empty() {
                    bail!("input JSON array contains no documents");
                }
                return Ok(values);
            }
            _ => bail!("input JSON document must be an object, array of objects, or JSONL rows"),
        }
    }

    let mut rows = Vec::new();
    for (line_index, line) in input_text.lines().enumerate() {
        let line_number = line_index + 1;
        if line.trim().is_empty() {
            continue;
        }
        rows.push(
            serde_json::from_str(line)
                .with_context(|| format!("invalid JSONL object at line {line_number}"))?,
        );
    }

    if rows.is_empty() {
        bail!("input contains no JSONL rows");
    }

    Ok(rows)
}

pub(super) fn parse_import_time(value: Option<&str>) -> Result<DateTime<Utc>> {
    match value {
        Some(value) => Ok(DateTime::parse_from_rfc3339(value)
            .with_context(|| format!("invalid --import-time {value:?}; expected RFC3339"))?
            .with_timezone(&Utc)),
        None => Ok(Utc::now()),
    }
}

pub(super) fn default_source_artifact_ref(input: &Path) -> String {
    input
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.is_empty())
        .unwrap_or("livekit-tool-action.json")
        .to_string()
}

pub(super) fn sha256_file(path: &Path) -> Result<String> {
    let file = File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}
