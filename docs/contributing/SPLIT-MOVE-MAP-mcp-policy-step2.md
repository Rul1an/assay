# SPLIT-MOVE-MAP — Wave15 Step2 — `mcp/policy`

## Goal

Mechanical split of `crates/assay-core/src/mcp/policy.rs` into bounded modules under `crates/assay-core/src/mcp/policy/` with no behavior/API change.

## New layout

- `crates/assay-core/src/mcp/policy/mod.rs`
  - facade: public types, serde compatibility glue, and thin wrappers
- `crates/assay-core/src/mcp/policy/engine.rs`
  - policy evaluation path (`evaluate_with_metadata`, allow/deny/class matching, rate-limit checks, request `check`)
- `crates/assay-core/src/mcp/policy/schema.rs`
  - schema compilation + legacy constraints-to-schema migration helpers
- `crates/assay-core/src/mcp/policy/legacy.rs`
  - file loading, legacy shape normalization, v1 detection, deprecation warning, cross-validation
- `crates/assay-core/src/mcp/policy/response.rs`
  - deny response formatter (`make_deny_response`), re-exported by facade

## Item mapping

- `impl McpPolicy::evaluate_with_metadata` body
  - from old `policy.rs` -> `engine::evaluate_with_metadata`
- `impl McpPolicy::check` body
  - from old `policy.rs` -> `engine::check`
- rate-limit + allow/deny + class matching helpers
  - from old `policy.rs` private methods -> `engine.rs` private helpers
- `impl McpPolicy::compile_all_schemas` body
  - from old `policy.rs` -> `schema::compile_all_schemas`
- `constraint_to_schema`
  - from old `policy.rs` -> `schema.rs` private helper
- `from_file` / `validate` / `is_v1_format` / `normalize_legacy_shapes`
  - from old `policy.rs` -> `legacy.rs`
- `emit_deprecation_warning`
  - from old `policy.rs` -> `legacy.rs` private helper
- `make_deny_response`
  - from old `policy.rs` -> `response.rs` (re-exported in `mod.rs`)

## Public-surface parity

- `McpPolicy` / `PolicyState` / `PolicyDecision` and related public structs/enums remain under `mcp::policy`.
- `mcp::policy::make_deny_response` path remains valid via re-export.
- No new public API symbols outside existing policy surface.

## Behavior parity notes

- Legacy normalization/migration order is preserved: parse -> warn (if v1) -> normalize -> migrate constraints -> validate.
- Decision codes/messages and deny-contract payload shape remain unchanged.
- Schema compilation still panics on invalid schema at load-time (secure fail behavior unchanged).
