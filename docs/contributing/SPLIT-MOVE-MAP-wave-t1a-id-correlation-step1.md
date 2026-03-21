# SPLIT MOVE MAP — Wave T1a Id Correlation Step1

## Intent
This step hardens the shared JSON-RPC id normalization and correlation contract. It does not add transport behavior or new runtime semantics.

## Data flow map
1. `crates/assay-core/src/mcp/parser.rs`
   - normalizes JSON-RPC ids into the accepted set
   - rejects invalid id types
   - rejects duplicate `tools/call` request ids before mapping
2. `crates/assay-core/tests/mcp_id_correlation.rs`
   - freezes parser and correlation behavior for string/numeric/null/missing/invalid ids
   - freezes first-match-wins and orphan behavior
3. `crates/assay-cli/tests/mcp_id_correlation_errors.rs`
   - verifies user-facing parse failures stay understandable
4. `docs/mcp/import-formats.md`
   - records the JSON-RPC id normalization contract

## Reviewer focus
- JSON null never becomes literal string `"null"`
- duplicate request ids fail in parser scope rather than creating silent correlation ambiguity
- first matching response still wins without mapper churn
- no transport or session semantics leak into this slice
