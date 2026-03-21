# Wave T1 Step1 inventory: transport transcript compatibility

Snapshot baseline (`origin/main` before Step1): `e84dcfc5`
Working branch head: see `git rev-parse --short HEAD`

Target files:
- `crates/assay-core/src/mcp/types.rs`
- `crates/assay-core/src/mcp/parser.rs`
- `crates/assay-core/tests/mcp_transport_compat.rs`
- `crates/assay-cli/src/cli/args/import.rs`
- `crates/assay-cli/src/cli/args/replay.rs`
- `crates/assay-cli/src/cli/commands/import.rs`
- `crates/assay-cli/src/cli/commands/trace.rs`
- `crates/assay-cli/tests/mcp_transport_import.rs`
- `docs/mcp/import-formats.md`
- `scripts/ci/review-wave-t1-transport-compat-step1.sh`
- `docs/contributing/SPLIT-*wave-t1-transport-compat-step1.md`

Step1 contract:
- Add `streamable-http` and `http-sse` transcript imports without changing the runtime MCP transport stack.
- Normalize `request`, `response`, and JSON-RPC-carrying `sse.data` through one MCP parser path.
- Keep canonical semantic equivalence across `jsonrpc`, `streamable-http`, and `http-sse`.
- Preserve transport context in envelopes, but do not promote it into V2 trace fields.

Acceptance anchors:
- canonical semantic equivalence covers:
  - event count
  - event kind order
  - JSON-RPC correlation
  - tool name
  - args
  - result or error
  - orphan response behavior
- legacy `endpoint` events do not create false-positive tool semantics
- `sse-legacy` remains an alias for the documented `http-sse` CLI format

Non-goals in Step1:
- no live HTTP client/server behavior
- no session lifecycle validation
- no `Mcp-Session-Id` semantics
- no `Last-Event-ID` replay/resume semantics
- no multi-stream SSE correlation
- no origin/auth/runtime enforcement changes
