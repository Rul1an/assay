# Wave T1 Step1 checklist: transport transcript compatibility

Scope freeze:
- [ ] Transcript import/parser/docs/tests only; no live transport runtime work.
- [ ] No MCP runtime proxy or enforcement changes.
- [ ] No V2 trace schema changes.

Files:
- [ ] `crates/assay-core/src/mcp/types.rs`
- [ ] `crates/assay-core/src/mcp/parser.rs`
- [ ] `crates/assay-core/tests/mcp_transport_compat.rs`
- [ ] `crates/assay-cli/src/cli/args/import.rs`
- [ ] `crates/assay-cli/src/cli/args/replay.rs`
- [ ] `crates/assay-cli/src/cli/commands/import.rs`
- [ ] `crates/assay-cli/src/cli/commands/trace.rs`
- [ ] `crates/assay-cli/tests/mcp_transport_import.rs`
- [ ] `docs/mcp/import-formats.md`
- [ ] `scripts/ci/review-wave-t1-transport-compat-step1.sh`
- [ ] `docs/contributing/SPLIT-*wave-t1-transport-compat-step1.md`

Parser and contract anchors:
- [ ] `McpInputFormat` includes `StreamableHttp` and `HttpSse`
- [ ] CLI accepts `streamable-http` and `http-sse`
- [ ] CLI accepts `sse-legacy` as an alias for `http-sse`
- [ ] `streamable-http` and `http-sse` envelopes require exactly one of `request`, `response`, or `sse` per entry
- [ ] `sse.data` accepts object or string payloads
- [ ] `event == "message"` may carry JSON-RPC semantics
- [ ] legacy `endpoint` and other transport-only SSE events stay out of tool/evidence semantics
- [ ] transport context is accepted in the envelope but not promoted into V2 trace fields
- [ ] protocol-version transport context changes do not alter semantic equivalence

Validation:
- [ ] `cargo fmt --check`
- [ ] `cargo clippy -q -p assay-core -p assay-cli --all-targets -- -D warnings`
- [ ] `cargo test -q -p assay-core --test mcp_transport_compat`
- [ ] `cargo test -q -p assay-core --test mcp_import_smoke`
- [ ] `cargo test -q -p assay-cli --test mcp_transport_import`
- [ ] reviewer script passes against `origin/main`
- [ ] `git diff --check`
