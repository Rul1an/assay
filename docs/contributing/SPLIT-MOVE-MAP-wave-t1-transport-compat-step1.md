# SPLIT MOVE MAP — Wave T1 Transport Compatibility Step1

## Intent
This step is parser and transcript normalization work only. There is no runtime transport abstraction layer and no proxy/runtime behavior move.

## Data flow map
1. `crates/assay-core/src/mcp/types.rs`
   - extends `McpInputFormat` with `StreamableHttp` and `HttpSse`
   - centralizes CLI label parsing, including `sse-legacy`
2. `crates/assay-core/src/mcp/parser.rs`
   - refactors JSON-RPC object parsing into a shared helper
   - adds transport-envelope flatteners for `streamable-http` and `http-sse`
   - ignores transport-only SSE control events for tool/evidence semantics
3. `crates/assay-core/tests/mcp_transport_compat.rs`
   - locks parser contracts and canonical semantic equivalence across transport families
4. `crates/assay-cli/src/cli/commands/import.rs`
   - wires new formats into `assay import`
5. `crates/assay-cli/src/cli/commands/trace.rs`
   - wires new formats into `assay trace import-mcp`
6. `crates/assay-cli/tests/mcp_transport_import.rs`
   - proves both CLI surfaces write valid traces for the new formats
7. `docs/mcp/import-formats.md`
   - documents modern `streamable-http`, compatibility-only `http-sse`, and T1 scope boundaries

## Reviewer focus
- Transport-context fields remain accepted but semantically inert in T1
- No new runtime transport or session-state code leaks into this slice
- Legacy SSE endpoint/control events do not perturb tool-call meaning
- The new CLI labels and envelope naming stay frozen and documented
