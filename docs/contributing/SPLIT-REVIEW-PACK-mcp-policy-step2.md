# MCP Policy Step2 Review Pack (Mechanical Split)

## Intent

Perform Wave15 mechanical split of `crates/assay-core/src/mcp/policy.rs` into focused modules while preserving policy behavior and public surface.

## Scope

- `crates/assay-core/src/mcp/policy.rs` (deleted)
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/policy/engine.rs`
- `crates/assay-core/src/mcp/policy/schema.rs`
- `crates/assay-core/src/mcp/policy/legacy.rs`
- `crates/assay-core/src/mcp/policy/response.rs`
- `docs/contributing/SPLIT-CHECKLIST-mcp-policy-step2.md`
- `docs/contributing/SPLIT-MOVE-MAP-mcp-policy-step2.md`
- `docs/contributing/SPLIT-REVIEW-PACK-mcp-policy-step2.md`
- `scripts/ci/review-mcp-policy-step2.sh`

## Non-goals

- no workflow changes
- no policy contract redesign
- no new behavior in allow/deny/schema evaluation

## Validation

```bash
BASE_REF=origin/main bash scripts/ci/review-mcp-policy-step2.sh
```

Gate includes:

```bash
cargo fmt --check
cargo clippy -p assay-core --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core test_mixed_tools_config -- --exact
```

## Reviewer 60s scan

1. Confirm diff stays in Step2 allowlist.
2. Confirm facade wrappers in `mod.rs` delegate to `engine`/`schema`/`legacy`.
3. Confirm no workflows changed.
4. Confirm targeted tests remain green.
5. Confirm `make_deny_response` still resolves at `mcp::policy::make_deny_response`.
