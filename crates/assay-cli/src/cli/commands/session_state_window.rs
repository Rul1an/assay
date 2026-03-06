use crate::exit_codes::{EXIT_CONFIG_ERROR, EXIT_INFRA_ERROR, EXIT_SUCCESS};
use jsonschema::{Draft, Validator};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::OnceLock;

fn session_state_window_schema_json() -> &'static str {
    include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/schemas/session_state_window_v1.schema.json"
    ))
}

fn session_state_window_validator() -> Result<&'static Validator, String> {
    static COMPILED: OnceLock<Result<Validator, String>> = OnceLock::new();
    static VALIDATOR: OnceLock<Validator> = OnceLock::new();

    let compiled = COMPILED.get_or_init(|| {
        let schema: Value = serde_json::from_str(session_state_window_schema_json())
            .map_err(|e| format!("failed to parse embedded session_state_window schema: {e}"))?;

        jsonschema::options()
            .with_draft(Draft::Draft202012)
            .build(&schema)
            .map_err(|e| format!("failed to compile session_state_window schema: {e}"))
    });

    match compiled {
        Ok(validator) => Ok(VALIDATOR.get_or_init(|| validator.clone())),
        Err(err) => Err(err.clone()),
    }
}

fn validate_session_state_window_v1(report: &Value) -> Result<(), String> {
    let validator = session_state_window_validator()?;
    let mut errs = validator.iter_errors(report);
    let mut out = Vec::new();

    for _ in 0..8 {
        if let Some(err) = errs.next() {
            out.push(format!("{err} at {}", err.instance_path()));
        } else {
            break;
        }
    }

    if out.is_empty() {
        Ok(())
    } else {
        Err(out.join("; "))
    }
}

fn canonical_json_bytes(v: &Value) -> anyhow::Result<Vec<u8>> {
    fn normalize(v: &Value) -> Value {
        match v {
            Value::Object(map) => {
                let mut keys: Vec<_> = map.keys().cloned().collect();
                keys.sort();
                let mut out = serde_json::Map::new();
                for key in keys {
                    out.insert(key.clone(), normalize(&map[&key]));
                }
                Value::Object(out)
            }
            Value::Array(arr) => Value::Array(arr.iter().map(normalize).collect()),
            _ => v.clone(),
        }
    }

    Ok(serde_json::to_vec(&normalize(v))?)
}

fn digest_canonical_json(v: &Value) -> anyhow::Result<String> {
    let mut hasher = Sha256::new();
    hasher.update(canonical_json_bytes(v)?);
    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

pub(crate) async fn write_state_window_out(
    out: &Path,
    event_source: &str,
    server_id: &str,
    session_id: &str,
) -> anyhow::Result<i32> {
    let privacy = json!({
        "stores_raw_tool_args": false,
        "stores_raw_prompt_bodies": false,
        "stores_raw_document_bodies": false
    });

    let snapshot_payload = json!({
        "session": {
            "event_source": event_source,
            "server_id": server_id,
            "session_id": session_id
        },
        "window": {
            "window_kind": "session"
        },
        "privacy": privacy
    });

    let state_snapshot_id = match digest_canonical_json(&snapshot_payload) {
        Ok(id) => id,
        Err(e) => {
            eprintln!("Measurement error: failed to compute state snapshot id: {e}");
            return Ok(EXIT_CONFIG_ERROR);
        }
    };

    let report = json!({
        "schema_version": "session_state_window_v1",
        "report_version": "1",
        "session": {
            "event_source": event_source,
            "server_id": server_id,
            "session_id": session_id
        },
        "window": {
            "window_kind": "session"
        },
        "snapshot": {
            "state_snapshot_id": state_snapshot_id,
            "canonicalization": {
                "method": "canonical_json_sha256"
            }
        },
        "privacy": privacy
    });

    if let Err(e) = validate_session_state_window_v1(&report) {
        eprintln!("Measurement error: session state window schema validation failed: {e}");
        return Ok(EXIT_CONFIG_ERROR);
    }

    let Some(parent) = out.parent() else {
        eprintln!("Infra error: invalid output path {}", out.display());
        return Ok(EXIT_INFRA_ERROR);
    };

    if !parent.as_os_str().is_empty() {
        if let Err(e) = tokio::fs::create_dir_all(parent).await {
            eprintln!("Infra error: failed to prepare {}: {e}", parent.display());
            return Ok(EXIT_INFRA_ERROR);
        }
    }

    let payload = serde_json::to_vec_pretty(&report)
        .expect("session state window report serialization should be infallible");
    if let Err(e) = tokio::fs::write(out, payload).await {
        eprintln!(
            "Infra error: failed to write state window report to {}: {e}",
            out.display()
        );
        return Ok(EXIT_INFRA_ERROR);
    }

    eprintln!("Wrote session_state_window_v1 to {}", out.display());

    Ok(EXIT_SUCCESS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;
    use tempfile::tempdir;

    #[tokio::test]
    async fn state_window_writer_emits_schema_valid_session_report() {
        let dir = tempdir().unwrap();
        let out = dir.path().join("state.json");

        let exit = write_state_window_out(
            &out,
            "assay://tests/session-state",
            "default-mcp-server",
            "mcpwrap-123",
        )
        .await
        .unwrap();

        assert_eq!(exit, EXIT_SUCCESS);

        let report: Value = serde_json::from_str(&std::fs::read_to_string(&out).unwrap()).unwrap();
        assert_eq!(report["schema_version"], "session_state_window_v1");
        assert_eq!(report["window"]["window_kind"], "session");
        assert_eq!(report["privacy"]["stores_raw_tool_args"], false);
        assert_eq!(report["privacy"]["stores_raw_prompt_bodies"], false);
        assert_eq!(report["privacy"]["stores_raw_document_bodies"], false);

        let id = report["snapshot"]["state_snapshot_id"].as_str().unwrap();
        let re = Regex::new(r"^sha256:[0-9a-f]{64}$").unwrap();
        assert!(re.is_match(id));
    }
}
