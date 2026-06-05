# SPLIT MOVE MAP - Wave53 Step5 - Policy Facade Closure

## Stack Base

Step5 is stacked on Step4:

- base: `codex/wave53-hotspot-top2-9-step4`
- head: `codex/wave53-hotspot-top2-9-step5`

Review Step5 against the Step4 branch, not directly against `main`, so earlier Wave53 movement does
not obscure the policy facade split.

## Mechanical Movement

Facade:

- `crates/assay-core/src/mcp/policy/mod.rs`

Moved implementation:

- `crates/assay-core/src/mcp/policy/types.rs`
- `crates/assay-core/src/mcp/policy/deserialize.rs`
- `crates/assay-core/src/mcp/policy/matcher.rs`
- `crates/assay-core/src/mcp/policy/contracts.rs`

`mod.rs` keeps the public policy module surface, `McpPolicy` inherent methods, and stable re-exports.
The child modules own the moved public data types, legacy constraints deserializer, pattern matcher,
typed decision contract mapping, and moved contract tests.

## Explicit Non-Movement

- No edits under `.github/workflows/**`.
- No edits under `crates/assay-core/src/mcp/policy/engine_next/**`.
- No policy-engine redesign, reason-code changes, decision semantic changes, or YAML/JSON shape
  changes.
- No Step2, Step3, or Step4 source target edits.

## LOC Snapshot

| Area | Before facade LOC | After facade LOC | New implementation modules |
| --- | ---: | ---: | --- |
| `mcp/policy/mod.rs` | 636 | 92 | `types.rs` 313, `contracts.rs` 174, `deserialize.rs` 49, `matcher.rs` 29 |
