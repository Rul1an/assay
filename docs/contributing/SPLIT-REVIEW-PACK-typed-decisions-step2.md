# SPLIT REVIEW PACK — Wave24 Typed Decisions Step2

## Intent
Implement Wave24 contract upgrades in bounded runtime scope:
- typed decisions
- Decision Event v2
- `AllowWithWarning` compatibility

No execution-semantics expansion is allowed in this slice.

## Scope
- `crates/assay-core/src/mcp/**`
- `crates/assay-core/tests/{decision_emit_invariant.rs,tool_taxonomy_policy_match.rs}`
- optional compat:
  - `crates/assay-cli/src/cli/commands/{mcp.rs,session_state_window.rs,coverage/**}`
  - `crates/assay-mcp-server/{src/auth.rs,tests/auth_integration.rs}`
- Step2 docs/gate:
  - `docs/contributing/SPLIT-CHECKLIST-typed-decisions-step2.md`
  - `docs/contributing/SPLIT-MOVE-MAP-typed-decisions-step2.md`
  - `docs/contributing/SPLIT-REVIEW-PACK-typed-decisions-step2.md`
  - `scripts/ci/review-wave24-typed-decisions-step2.sh`

## Non-goals
- no obligations execution
- no approval enforcement
- no policy-backend rewrite
- no auth transport redesign
- no workflow edits

## Validation
```bash
BASE_REF=origin/codex/wave24-typed-decisions-step1-freeze bash scripts/ci/review-wave24-typed-decisions-step2.sh
```

Gate includes:
```bash
cargo fmt --check
cargo clippy -p assay-core -p assay-cli -p assay-mcp-server --all-targets -- -D warnings
cargo test -p assay-core tool_taxonomy_policy_match_handler_decision_event_records_classes -- --exact
cargo test -p assay-core test_event_contains_required_fields -- --exact
cargo test -p assay-core decision_emit_invariant
cargo test -p assay-cli mcp_wrap_coverage
cargo test -p assay-cli mcp_wrap_state_window_out
cargo test -p assay-mcp-server auth_integration
```

## Reviewer 60s scan
1. Confirm diff remains inside Step2 allowlist.
2. Confirm typed decision markers exist (`allow_with_obligations`, `deny_with_alert`).
3. Confirm `AllowWithWarning` compatibility path remains.
4. Confirm Decision Event v2 markers are present.
5. Confirm no obligations execution markers appear in runtime scope.
