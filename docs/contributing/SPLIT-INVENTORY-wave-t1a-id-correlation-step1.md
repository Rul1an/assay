# Wave T1a Step1 inventory: JSON-RPC id normalization and correlation

Snapshot baseline (`origin/main` before Step1): `d90ca7d5`
Working branch head: see `git rev-parse --short HEAD`

Target files:
- `crates/assay-core/src/mcp/parser.rs`
- `crates/assay-core/tests/mcp_id_correlation.rs`
- `crates/assay-core/tests/mcp_transport_compat.rs`
- `crates/assay-cli/tests/mcp_id_correlation_errors.rs`
- `docs/mcp/import-formats.md`
- `scripts/ci/review-wave-t1a-id-correlation-step1.sh`
- `docs/contributing/SPLIT-*wave-t1a-id-correlation-step1.md`

Step1 contract:
- Normalize JSON-RPC ids as:
  - string -> string
  - numeric -> canonical string
  - null -> none
  - missing -> none
- Reject invalid id types:
  - bool
  - object
  - array
- Reject duplicate `tools/call` request ids inside one transcript.
- Preserve first-match-wins response binding; later duplicate responses remain orphan and do not overwrite prior correlation.

Non-goals in Step1:
- no extra transport compatibility work
- no session or resumability work
- no multi-stream handling
- no broad `mapper_v2.rs` refactor
- no runtime MCP behavior changes
