# Fuzz Targets

This directory contains `cargo-fuzz` harnesses for parser and bundle-reader surfaces
that are easy to regress silently:

- `policy_yaml`: fuzzes YAML policy parsing for both eval config and MCP policy shapes
- `bundle_reader`: fuzzes evidence-chain verification against arbitrary tar.gz bytes under small,
  explicit resource ceilings; deterministic fail-closed classifications live in
  `crates/assay-evidence/tests/verifier_fail_closed_properties.rs`
- `mcp_jsonrpc`: fuzzes the stdio JSON-RPC line handler through the same `handle_line` the
  `assay-mcp-server` loop calls, asserting notification silence, id echo, and bounded responses;
  the deterministic contract frames live in `server::line_handler_tests`
- `tool_call_decision`: fuzzes the `tools/call` envelope classifier and the observed
  tool-decision builder, asserting totality, determinism, and argument redaction

Examples:

```bash
cd fuzz
cargo fuzz run policy_yaml
cargo fuzz run bundle_reader
cargo fuzz run mcp_jsonrpc
cargo fuzz run tool_call_decision
```
