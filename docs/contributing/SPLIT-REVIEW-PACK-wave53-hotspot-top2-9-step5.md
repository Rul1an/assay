# SPLIT REVIEW PACK - Wave53 Step5 - Policy Facade Closure

## Scope

Step5 mechanically splits the MCP policy module hotspot behind the stable `policy/mod.rs` facade:

- `crates/assay-core/src/mcp/policy/mod.rs`

This PR should be reviewed as a stacked PR on `codex/wave53-hotspot-top2-9-step4`.

## Files

- `docs/contributing/SPLIT-PLAN-wave53-hotspot-top2-9.md`
- `docs/contributing/SPLIT-CHECKLIST-wave53-hotspot-top2-9-step5.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave53-hotspot-top2-9-step5.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave53-hotspot-top2-9-step5.md`
- `scripts/ci/review-wave53-hotspot-top2-9-step5.sh`
- `crates/assay-core/src/mcp/policy/mod.rs`
- `crates/assay-core/src/mcp/policy/types.rs`
- `crates/assay-core/src/mcp/policy/deserialize.rs`
- `crates/assay-core/src/mcp/policy/matcher.rs`
- `crates/assay-core/src/mcp/policy/contracts.rs`

## Verification

Run the Step5 gate from the Step5 branch:

```bash
bash scripts/ci/review-wave53-hotspot-top2-9-step5.sh
```

The gate checks the stack diff against `codex/wave53-hotspot-top2-9-step4` by default. Use
`BASE_REF=<ref>` only when reviewing a differently named local stack base.

The gate runs:

```bash
cargo fmt --check
cargo check -p assay-core
cargo test -q -p assay-core --test policy_engine_test
cargo test -q -p assay-core --lib policy
cargo clippy -p assay-core --all-targets -- -D warnings
git diff --check
```

## Reviewer Focus

- Confirm `policy/mod.rs` preserves the public facade and `McpPolicy` method surface.
- Confirm YAML/JSON constraints deserialization still accepts the legacy list and map shapes.
- Confirm tool-pattern matching behavior and policy reason-code strings are moved without changes.
- Confirm typed policy decision contracts still map warning and alert obligations identically.
- Confirm `engine_next/*` is untouched and Step5 does not redesign policy evaluation behavior.

## LOC Deltas

| File | Before LOC | After LOC |
| --- | ---: | ---: |
| `crates/assay-core/src/mcp/policy/mod.rs` | 636 | 92 |

Moved code now lives in:

| File | LOC |
| --- | ---: |
| `crates/assay-core/src/mcp/policy/types.rs` | 313 |
| `crates/assay-core/src/mcp/policy/contracts.rs` | 174 |
| `crates/assay-core/src/mcp/policy/deserialize.rs` | 49 |
| `crates/assay-core/src/mcp/policy/matcher.rs` | 29 |

## Residual Hotspots

Wave53 closes the selected top 2 through 9 files behind stable facades. Residual policy hotspots are
deliberately deferred: `policy/engine_next/effects.rs` remains behavior-heavy but is out of scope for
this mechanical facade closure.

## PR Timing

Open Step5 only after this gate passes locally and Step4 is green. Step5 is the final closure PR in
the Wave53 top 2 through 9 stack.
