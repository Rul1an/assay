# Wave T1a Step1 checklist: JSON-RPC id normalization and correlation

Scope freeze:
- [ ] Parser/correlation hardening only; no transport expansion.
- [ ] No live runtime changes.
- [ ] No V2 schema change.

Files:
- [ ] `crates/assay-core/src/mcp/parser.rs`
- [ ] `crates/assay-core/tests/mcp_id_correlation.rs`
- [ ] `crates/assay-core/tests/mcp_transport_compat.rs`
- [ ] `crates/assay-cli/tests/mcp_id_correlation_errors.rs`
- [ ] `docs/mcp/import-formats.md`
- [ ] `scripts/ci/review-wave-t1a-id-correlation-step1.sh`
- [ ] `docs/contributing/SPLIT-*wave-t1a-id-correlation-step1.md`

Contract anchors:
- [ ] string ids remain strings
- [ ] numeric ids normalize to strings
- [ ] JSON null ids normalize to none
- [ ] missing ids normalize to none
- [ ] literal string `"null"` is not produced from JSON null
- [ ] bool ids fail hard
- [ ] object ids fail hard
- [ ] array ids fail hard
- [ ] duplicate `tools/call` request ids fail hard
- [ ] first matching response binds the request
- [ ] later responses with the same id remain orphan and do not overwrite earlier correlation

Validation:
- [ ] `cargo fmt --check`
- [ ] `cargo clippy -q -p assay-core -p assay-cli --all-targets -- -D warnings`
- [ ] `cargo test -q -p assay-core --test mcp_id_correlation`
- [ ] `cargo test -q -p assay-core --test mcp_transport_compat`
- [ ] `cargo test -q -p assay-core --test mcp_import_smoke`
- [ ] `cargo test -q -p assay-cli --test mcp_id_correlation_errors`
- [ ] reviewer script passes against `origin/main`
- [ ] `git diff --check`
