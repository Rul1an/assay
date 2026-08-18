use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

// Assay-owned application codes. `data.origin` marks them so they are distinguishable from
// upstream errors regardless of code. PROXY_DENIED is a P61e enforcing-mode policy denial;
// the `reason` is the precedence-pinned gate that fired.
pub(super) use super::error_codes::{PROXY_DENIED, PROXY_FAILED, PROXY_UNSUPPORTED};

pub(super) fn proxy_error_line(id: Value, code: i32, reason: &str, message: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message,
            "data": { "origin": "assay-proxy", "reason": reason }
        }
    })
    .to_string()
}

/// Append one compact JSON object as an NDJSON line. The enforcing proxy uses this for the canonical
/// decision record and sibling carriers; callers decide whether a write failure is fail-closed.
pub(super) fn append_decision_record(path: &Path, record: &Value) -> std::io::Result<()> {
    use std::io::Write as _;
    let line = serde_json::to_string(record)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    writeln!(f, "{line}")
}

/// Atomic-ish write: serialize to a sibling temp file in the same directory, then rename over the
/// destination. On Windows `rename` will not replace an existing file, so fall back to remove+rename.
pub(super) fn write_json_atomic(path: &Path, value: &Value) -> std::io::Result<()> {
    let bytes = serde_json::to_vec_pretty(value)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("artifact");
    let tmp_name = format!("{file_name}.tmp.{}", std::process::id());
    let tmp = match path.parent() {
        Some(p) if !p.as_os_str().is_empty() => p.join(tmp_name),
        _ => PathBuf::from(tmp_name),
    };
    std::fs::write(&tmp, &bytes)?;
    match std::fs::rename(&tmp, path) {
        Ok(()) => Ok(()),
        Err(_) if path.exists() => {
            std::fs::remove_file(path)?;
            std::fs::rename(&tmp, path)
        }
        Err(e) => {
            let _ = std::fs::remove_file(&tmp);
            Err(e)
        }
    }
}

pub(super) async fn forward_line<W: AsyncWriteExt + Unpin>(
    w: &mut W,
    line: &str,
) -> std::io::Result<()> {
    w.write_all(line.as_bytes()).await?;
    w.write_all(b"\n").await?;
    w.flush().await
}

/// Upstream unavailable: never forward, never fabricate. Answer every client request with a
/// proxy-originated failure until the client closes stdin.
pub(super) async fn degraded_loop(reason: &str) {
    let mut out = tokio::io::stdout();
    let mut client = BufReader::new(tokio::io::stdin()).lines();
    while let Ok(Some(line)) = client.next_line().await {
        if line.trim().is_empty() {
            continue;
        }
        let v: Value = match serde_json::from_str(&line) {
            Ok(v) => v,
            Err(_) => continue,
        };
        if let Some(id) = v.get("id") {
            if !id.is_null() {
                let msg = proxy_error_line(
                    id.clone(),
                    PROXY_FAILED,
                    reason,
                    "upstream MCP server is unavailable",
                );
                let _ = out.write_all(msg.as_bytes()).await;
                let _ = out.write_all(b"\n").await;
                let _ = out.flush().await;
            }
        }
    }
}
