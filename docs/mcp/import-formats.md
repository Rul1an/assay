# Import Formats

Supported MCP transcript formats for importing traces into Assay.

---

## Overview

Assay can import MCP sessions from these sources:

| Format | Source | Status |
|--------|--------|--------|
| `inspector` | [MCP Inspector](https://github.com/modelcontextprotocol/inspector) | Supported |
| `jsonrpc` | Raw JSON-RPC 2.0 messages | Supported |
| `streamable-http` | Modern MCP Streamable HTTP transcript capture | Supported |
| `http-sse` | Assay import label for deprecated MCP HTTP+SSE captures | Supported |

`streamable-http` is the modern HTTP baseline in the MCP transports specification.
`http-sse` is an Assay compatibility label for the deprecated legacy HTTP+SSE transport family.
Assay also accepts `sse-legacy` as a CLI alias for `http-sse`, but only `http-sse` is documented as the canonical import label.

---

## CLI Usage

Two CLI surfaces accept the same MCP import formats:

```bash
assay import session.json --format streamable-http --out-trace session.trace.jsonl
assay trace import-mcp --input session.json --format http-sse --out-trace session.trace.jsonl
```

Supported values:

- `inspector`
- `jsonrpc`
- `streamable-http`
- `http-sse`

Compatibility alias:

- `sse-legacy` maps to `http-sse`

---

## Canonical Transport Envelope

Assay uses one canonical JSON envelope for both HTTP-based transcript imports.
The CLI labels stay kebab-case, and the envelope `transport` field uses the same kebab-case values.

```json
{
  "transport": "streamable-http",
  "transport_context": {
    "headers": {
      "MCP-Protocol-Version": "2025-06-18",
      "Mcp-Session-Id": "session-123"
    }
  },
  "entries": [
    {
      "timestamp_ms": 1000,
      "request": {
        "jsonrpc": "2.0",
        "id": "1",
        "method": "tools/call",
        "params": {
          "name": "lookup_customer",
          "arguments": { "id": "cust_123" }
        }
      }
    },
    {
      "timestamp_ms": 1001,
      "response": {
        "jsonrpc": "2.0",
        "id": "1",
        "result": { "name": "Alice" }
      }
    },
    {
      "timestamp_ms": 1002,
      "transport_context": {
        "headers": {
          "Last-Event-ID": "evt-9"
        }
      },
      "sse": {
        "event": "message",
        "id": "evt-10",
        "data": {
          "jsonrpc": "2.0",
          "method": "notifications/progress",
          "params": { "progress": 50 }
        }
      }
    }
  ]
}
```

Rules:

- Each `entries[]` item must contain exactly one of `request`, `response`, or `sse`.
- `request` is one client-to-server JSON-RPC message.
- `response` is one server-to-client JSON-RPC message delivered in an HTTP body.
- `sse.data` may be an object or a string. If it carries a JSON-RPC message, Assay normalizes it through the same JSON-RPC path as `request` and `response`.
- `sse.event == "message"` may contribute MCP semantics.
- Legacy control events such as `endpoint`, keepalives, and other transport-only SSE events are ignored for tool/evidence semantics.
- `transport_context`, `headers`, `MCP-Protocol-Version`, `Mcp-Session-Id`, and `Last-Event-ID` remain transport context by default.
- **`K2-A` exception:** on HTTP transcript imports, Assay may now promote one bounded authorization-discovery summary into `episode_start.meta.mcp.authorization_discovery`, but only from explicit `401` response context plus typed `WWW-Authenticate` challenge visibility (`resource_metadata` and/or `scope`).
- Outside that bounded `K2-A` exception, transport context still does not change MCP tool-call semantic equivalence assertions.

---

## Semantic Normalization Contract

T1 is a transcript compatibility wave. Its goal is canonical semantic equivalence:

- event count
- event kind order
- request/response correlation by JSON-RPC `id`
- tool name
- tool arguments
- tool result or error
- orphan response behavior

Equivalent sessions imported from `jsonrpc`, `streamable-http`, or `http-sse` should normalize to the same MCP tool semantics and the same Assay V2 tool-call meaning.

`K2-A` does **not** change that core guarantee. It only adds an optional bounded visibility summary on `episode_start.meta` when an HTTP transcript explicitly carries a `401` authorization challenge with typed discovery parameters.

### K2-A Authorization-Discovery Summary

When a supported HTTP transcript entry contains:

- a response-path `401` status in `transport_context`
- a typed `WWW-Authenticate` header
- and visible `resource_metadata` and/or `scope` challenge parameters

Assay may emit:

```json
{
  "meta": {
    "mcp": {
      "authorization_discovery": {
        "visible": true,
        "source_kind": "www_authenticate",
        "resource_metadata_visible": true,
        "authorization_servers_visible": false,
        "scope_challenge_visible": true
      }
    }
  }
}
```

This summary is **visibility-only**. It does not imply auth success, scope correctness, issuer trust, or MCP auth compliance.

---

## Streamable HTTP

Use `streamable-http` for captures based on the modern MCP HTTP transport model.

### JSON Response Example

```json
{
  "transport": "streamable-http",
  "entries": [
    {
      "timestamp_ms": 1000,
      "request": {
        "jsonrpc": "2.0",
        "id": "call-1",
        "method": "tools/call",
        "params": {
          "name": "Calculator",
          "arguments": { "a": 1, "b": 2 }
        }
      }
    },
    {
      "timestamp_ms": 1001,
      "response": {
        "jsonrpc": "2.0",
        "id": "call-1",
        "result": { "sum": 3 }
      }
    }
  ]
}
```

### SSE Response Example

```json
{
  "transport": "streamable-http",
  "entries": [
    {
      "timestamp_ms": 1000,
      "request": {
        "jsonrpc": "2.0",
        "id": "call-1",
        "method": "tools/call",
        "params": {
          "name": "Calculator",
          "arguments": { "a": 1, "b": 2 }
        }
      }
    },
    {
      "timestamp_ms": 1001,
      "sse": {
        "event": "message",
        "id": "evt-1",
        "data": {
          "jsonrpc": "2.0",
          "id": "call-1",
          "result": { "sum": 3 }
        }
      }
    }
  ]
}
```

---

## Legacy HTTP+SSE

Use `http-sse` for captured sessions from the deprecated MCP HTTP+SSE transport family.
This is an import compatibility label in Assay, not the modern transport name from the current MCP specification.

```json
{
  "transport": "http-sse",
  "entries": [
    {
      "timestamp_ms": 999,
      "sse": {
        "event": "endpoint",
        "id": "evt-0",
        "data": "/mcp/messages?session=legacy-session"
      }
    },
    {
      "timestamp_ms": 1000,
      "request": {
        "jsonrpc": "2.0",
        "id": "call-1",
        "method": "tools/call",
        "params": {
          "name": "Calculator",
          "arguments": { "a": 1, "b": 2 }
        }
      }
    },
    {
      "timestamp_ms": 1001,
      "sse": {
        "event": "message",
        "id": "evt-1",
        "data": "{\"jsonrpc\":\"2.0\",\"id\":\"call-1\",\"result\":{\"sum\":3}}"
      }
    }
  ]
}
```

The legacy `endpoint` event is treated as transport-only context and does not affect normalized tool semantics.

---

## JSON-RPC

`jsonrpc` remains the simplest raw import format for one-message-per-line JSON-RPC captures.

```json
{"jsonrpc":"2.0","id":"call-1","method":"tools/call","params":{"name":"Calculator","arguments":{"a":1,"b":2}}}
{"jsonrpc":"2.0","id":"call-1","result":{"sum":3}}
```

Assay correlates requests and responses by JSON-RPC `id`, just like the HTTP transcript formats.

### JSON-RPC `id` Normalization

For MCP import and correlation:

- string `id` values are accepted as-is
- numeric `id` values are accepted and normalized to strings
- `null` `id` values normalize to no correlation id
- missing `id` values also normalize to no correlation id
- JSON `null` is not treated as the literal string `"null"`
- boolean, object, and array `id` values are rejected as invalid input

Correlation notes:

- requests without a correlation id do not bind later responses
- the first matching response binds a request
- later responses with the same `id` stay orphan and do not overwrite the earlier match
- duplicate `tools/call` request ids in one transcript fail at parse time

---

## Out Of Scope In T1

T1 intentionally covers transcript compatibility only.
It does not implement or validate:

- live HTTP client/server behavior
- session lifecycle validation
- `Mcp-Session-Id` semantics
- `Last-Event-ID` replay or resumability semantics
- multi-stream SSE delivery correctness
- DELETE-based session termination
- origin, auth, or runtime security checks for live transports

Those concerns remain future transport/runtime work.
