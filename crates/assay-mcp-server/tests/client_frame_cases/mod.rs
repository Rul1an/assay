//! Shared wire-level admission regressions for wrap, observe and enforce.
use crate::jsonrpc_conn::Conn;
use std::path::Path;

pub fn assert_client_frame_admission(out: &mut Conn, raw: &Path, proxy: bool) {
    // Positive controls establish that the raw log exists and the upstream is alive.
    let response = r#"{"jsonrpc":"2.0","id":601,"result":{"nested":[1,[2]]},"opaque":true}"#;
    out.send_line(response);
    out.send_line(" \t\r");
    out.send_line(r#"{"jsonrpc":"2.0","id":602,"method":"ping"}"#);
    let ping = out.read_response();
    assert_eq!(ping["id"], 602, "blank lines must not produce errors");
    assert!(ping.get("error").is_none());
    let logged = std::fs::read_to_string(raw).expect("positive control must create raw log");
    assert!(logged.lines().any(|line| line == response));

    let error_response =
        r#"{"jsonrpc":"2.0","id":"legacy","error":{"code":-1,"message":"test","data":[1,[2]]}}"#;
    out.send_line(error_response);

    let cases = [
        (
            "denied batch",
            r#"[{"jsonrpc":"2.0","id":701,"method":"tools/call","params":{"name":"zz-batch-denied","arguments":{}}}]"#,
            false,
        ),
        (
            "allowed batch",
            r#"[{"jsonrpc":"2.0","id":702,"method":"tools/call","params":{"name":"echo","arguments":{}}}]"#,
            false,
        ),
        (
            "mixed batch",
            r#"[{"jsonrpc":"2.0","id":703,"method":"ping"},{"jsonrpc":"2.0","id":704,"method":"tools/call","params":{"name":"zz-batch-mixed"}}]"#,
            false,
        ),
        (
            "notification batch",
            r#"[{"jsonrpc":"2.0","method":"tools/call","params":{"name":"zz-batch-note"}}]"#,
            false,
        ),
        ("empty batch", "[]", false),
        ("nested batch", "[[{}]]", false),
        ("leading whitespace", " \t [ {} ]", false),
        ("string", r#""zz-frame-string""#, false),
        ("number", "17", false),
        ("boolean", "true", false),
        ("null", "null", false),
        (
            "positional request",
            r#"["2.0",705,"tools/call",{"name":"echo"}]"#,
            false,
        ),
        (
            "duplicate batch member",
            r#"[{"method":"ping","method":"tools/call","params":{"name":"zz-batch-duplicate"}}]"#,
            true,
        ),
        (
            "escaped duplicate",
            r#"[{"method":"ping","m\u0065thod":"tools/call"}]"#,
            true,
        ),
        (
            "malformed batch",
            r#"[{"method":"zz-batch-malformed"}"#,
            true,
        ),
        ("non-JSON", "zz-frame-not-json", true),
        (
            "null method",
            r#"{"method":null,"marker":"zz-frame-null-method"}"#,
            false,
        ),
        (
            "numeric method",
            r#"{"method":5,"marker":"zz-frame-number-method"}"#,
            false,
        ),
        (
            "array method",
            r#"{"method":["tools/call"],"marker":"zz-frame-array-method"}"#,
            false,
        ),
    ];
    for (name, frame, parse_error) in cases {
        assert_rejected(out, raw, name, frame, proxy, parse_error);
    }
    assert!(std::fs::read_to_string(raw)
        .unwrap()
        .lines()
        .any(|line| line == error_response));
    let limit = assay_core::mcp::ingest::McpTranscriptLimits::default().max_line_bytes as usize;
    let oversized = format!("[\"{}\"]", "x".repeat(limit));
    assert_rejected(out, raw, "oversized batch", &oversized, proxy, true);
}

fn assert_rejected(
    out: &mut Conn,
    raw: &Path,
    name: &str,
    frame: &str,
    proxy: bool,
    parse_error: bool,
) {
    out.send_line(frame);
    out.send_line(r#"{"jsonrpc":"2.0","id":799,"method":"ping"}"#);
    // On vulnerable code this is the ping response. Check the raw frame first so
    // RED proves forwarding, not merely a missing error or a crashed fixture.
    let rejection = out.read_response();
    let logged = std::fs::read_to_string(raw).expect("raw logging must remain available");
    assert!(
        !logged.lines().any(|line| line == frame.trim()),
        "{name}: rejected frame reached upstream"
    );
    assert!(rejection["id"].is_null(), "{name}: {rejection}");
    let code = if proxy {
        -31998
    } else if parse_error {
        -32700
    } else {
        -32600
    };
    assert_eq!(rejection["error"]["code"], code, "{name}: {rejection}");
    if proxy {
        assert_eq!(rejection["error"]["data"]["origin"], "assay-proxy");
    }
    let ping = out.read_response();
    assert_eq!(
        ping["id"], 799,
        "{name}: exactly one rejection before the ping"
    );
    assert!(
        ping.get("error").is_none(),
        "{name}: upstream must remain alive"
    );
    let logged = std::fs::read_to_string(raw).expect("raw log after upstream barrier");
    assert!(
        !logged.lines().any(|line| line == frame.trim()),
        "{name}: refused frame was forwarded before ping"
    );
}
