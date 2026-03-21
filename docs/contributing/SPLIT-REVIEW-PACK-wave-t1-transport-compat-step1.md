# Wave T1 Step1 review pack: transport transcript compatibility

## Scope summary
- Adds `streamable-http` and `http-sse` transcript imports to the MCP parser/import path.
- Keeps transport compatibility bounded to parser/import/docs/tests.
- Freezes `http-sse` as Assay's import label for deprecated MCP HTTP+SSE captures.

## What changed
- Shared JSON-RPC parser path now normalizes:
  - raw `jsonrpc`
  - `streamable-http` `request`
  - `streamable-http` `response`
  - JSON-RPC-bearing `sse.data`
- Transport-only SSE control events such as legacy `endpoint` remain out of tool/evidence semantics.
- CLI import surfaces now accept:
  - `streamable-http`
  - `http-sse`
  - `sse-legacy` alias

## Review questions
1. Do equivalent sessions across `jsonrpc`, `streamable-http`, and `http-sse` normalize to the same semantic tool trace?
2. Are transport-context fields accepted without leaking into trace semantics?
3. Is the scope still parser/import/docs/tests only?
4. Does the documentation make the legacy-vs-modern transport naming explicit enough?

## Validation commands
```bash
cargo fmt --check
cargo clippy -q -p assay-core -p assay-cli --all-targets -- -D warnings
cargo test -q -p assay-core --test mcp_transport_compat
cargo test -q -p assay-core --test mcp_import_smoke
cargo test -q -p assay-cli --test mcp_transport_import
BASE_REF=origin/main bash scripts/ci/review-wave-t1-transport-compat-step1.sh
git diff --check
```
