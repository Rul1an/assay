# Wave T1a Step1 review pack: JSON-RPC id normalization and correlation

## Scope summary
- Hardens the shared MCP JSON-RPC id contract without reopening T1 transport compatibility.
- Makes null/missing ids non-correlating, rejects invalid id types, and rejects duplicate `tools/call` request ids.
- Leaves transport, session, and runtime behavior out of scope.

## Review questions
1. Does JSON null now normalize to no id rather than literal `"null"`?
2. Are numeric ids still accepted and correlated canonically?
3. Do duplicate `tools/call` request ids fail before ambiguous mapping can happen?
4. Is first-match-wins behavior explicit and still stable?
5. Are CLI parse failures understandable for invalid id input?

## Validation commands
```bash
cargo fmt --check
cargo clippy -q -p assay-core -p assay-cli --all-targets -- -D warnings
cargo test -q -p assay-core --test mcp_id_correlation
cargo test -q -p assay-core --test mcp_transport_compat
cargo test -q -p assay-core --test mcp_import_smoke
cargo test -q -p assay-cli --test mcp_id_correlation_errors
BASE_REF=origin/main bash scripts/ci/review-wave-t1a-id-correlation-step1.sh
git diff --check
```
