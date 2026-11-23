use assay_core::mcp::unique_json::FrameDirection;
use serde_json::{json, Value};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufRead, AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};

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

pub(super) async fn next_frame(
    reader: &mut (impl AsyncBufRead + Unpin),
    direction: FrameDirection,
) -> std::io::Result<Option<String>> {
    let mut line = String::new();
    let n = reader
        .take(direction.content_limit() + 3)
        .read_line(&mut line)
        .await?;
    direction.check_wire_bytes(n)?;
    if n == 0 {
        return Ok(None);
    }
    // forward_line restores the transport terminator.
    if line.ends_with('\n') {
        line.pop();
        if line.ends_with('\r') {
            line.pop();
        }
    }
    Ok(Some(line))
}

/// Upstream unavailable: never forward, never fabricate. Answer every client request with a
/// proxy-originated failure until the client closes stdin.
pub(super) async fn degraded_loop(reason: &str) {
    let mut out = tokio::io::stdout();
    let mut client = BufReader::new(tokio::io::stdin());
    while let Ok(Some(line)) = next_frame(&mut client, FrameDirection::Client).await {
        if line.trim().is_empty() {
            continue;
        }
        let Some(msg) = degraded_response(&line, reason) else {
            continue;
        };
        let _ = out.write_all(msg.as_bytes()).await;
        let _ = out.write_all(b"\n").await;
        let _ = out.flush().await;
    }
}

fn degraded_response(line: &str, reason: &str) -> Option<String> {
    use assay_core::mcp::unique_json::{parse_client_frame, UniqueJsonError};
    match parse_client_frame(line) {
        Ok(v) => Some(proxy_error_line(
            v.get("id").filter(|id| !id.is_null())?.clone(),
            PROXY_FAILED,
            reason,
            "upstream MCP server is unavailable",
        )),
        Err(UniqueJsonError::InvalidRequestIdKnown(id)) => Some(proxy_error_line(
            id,
            PROXY_FAILED,
            "malformed_client_request",
            "client frame was rejected",
        )),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn degraded_reply_identifies_the_rejected_request() {
        let reply = degraded_response(r#"{"method":"ping","id":7}"#, "unavailable").unwrap();
        let reply: Value = serde_json::from_str(&reply).unwrap();
        assert_eq!(reply["id"], 7);
        assert_eq!(reply["error"]["data"]["reason"], "malformed_client_request");
        assert!(degraded_response(r#"{"method":"ping"}"#, "unavailable").is_none());
    }
}
