//! Shared wire-level admission regressions for wrap, observe and enforce.
use crate::jsonrpc_conn::Conn;
use std::path::Path;

pub fn assert_client_frame_admission(out: &mut Conn, raw: &Path, proxy: bool) {
    // Positive controls establish that the raw log exists and the upstream is alive.
    let response =
        r#"{"jsonrpc":"2.0","id":601,"result":{"nested":[1,[2]]},"opaque":true,"escaped":"\r\n"}"#;
    out.send_line(&format!("{response}\r"));
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
            "concatenated values",
            r#"{"result":{}}{"method":"tools/call"}"#,
            true,
        ),
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
        assert_rejected(out, raw, name, frame, proxy, parse_error, None);
    }
    assert!(std::fs::read_to_string(raw)
        .unwrap()
        .lines()
        .any(|line| line == error_response));
    // Python text readers split bare CR as a newline. A nested request must
    // never become a second upstream frame after authorization of its parent.
    for (name, frame) in [
        (
            "CR in ping",
            r#"{"jsonrpc":"2.0","id":900,"method":"ping","params":{"x":<CR>{"jsonrpc":"2.0","id":901,"method":"tools/call","params":{"name":"zz-cr-frame","arguments":{}}}<CR>}}"#,
        ),
        (
            "CR in response",
            r#"{"jsonrpc":"2.0","id":902,"result":<CR>{"jsonrpc":"2.0","id":903,"method":"tools/call","params":{"name":"zz-cr-frame","arguments":{}}}<CR>}"#,
        ),
    ] {
        assert_rejected(
            out,
            raw,
            name,
            &frame.replace("<CR>", "\r"),
            proxy,
            false,
            Some("zz-cr-frame"),
        );
    }
}

pub fn assert_oversized_frame_ends_transport(out: &mut Conn, raw: &Path) {
    let before = std::fs::read(raw).expect("prior liveness control reached the upstream");
    let limit = assay_core::mcp::ingest::McpTranscriptLimits::default().max_line_bytes as usize;
    let mut ping =
        serde_json::json!({"jsonrpc":"2.0","id":803,"method":"ping","params":{"note":""}});
    let padding = limit + 2 - ping.to_string().len();
    ping["params"]["note"] = serde_json::Value::String("x".repeat(padding));
    let line = ping.to_string();
    assert_eq!(
        line.len() + 1,
        limit + 3,
        "the final LF reaches the bounded read ceiling"
    );
    out.send_line(&line);
    // Leave stdin open so client EOF cannot cause the observed termination.
    assert!(
        !out.wait().success(),
        "oversized input must terminate the transport with an error"
    );
    assert_eq!(
        std::fs::read(raw).unwrap(),
        before,
        "oversized frame reached the upstream"
    );
}

fn assert_rejected(
    out: &mut Conn,
    raw: &Path,
    name: &str,
    frame: &str,
    proxy: bool,
    parse_error: bool,
    fragment: Option<&str>,
) {
    out.send_line(frame);
    let ping_id = format!("barrier-{name}");
    out.send(serde_json::json!({"jsonrpc":"2.0","id":ping_id,"method":"ping"}));
    // On vulnerable code this is the ping response. Check the raw frame first so
    // RED proves forwarding, not merely a missing error or a crashed fixture.
    let rejection = out.read_response();
    let logged = std::fs::read_to_string(raw).expect("raw logging must remain available");
    assert!(
        !fragment.map_or_else(
            || logged.lines().any(|line| line == frame.trim()),
            |needle| logged.contains(needle)
        ),
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
        ping["id"], ping_id,
        "{name}: exactly one rejection before the ping"
    );
    assert!(
        ping.get("error").is_none(),
        "{name}: upstream must remain alive"
    );
    let logged = std::fs::read_to_string(raw).expect("raw log after upstream barrier");
    assert!(
        !fragment.map_or_else(
            || logged.lines().any(|line| line == frame.trim()),
            |needle| logged.contains(needle)
        ),
        "{name}: refused frame was forwarded before ping"
    );
}
