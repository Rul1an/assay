# SPLIT PLAN - Wave 51 Hotspot Refactor 2026q2

## Intent

Reduce the four largest functional Rust hotspots behind stable public facades, with behavior freeze and reviewer gates before each move.

## Scope

- `crates/assay-core/src/engine/runner.rs`
- `crates/assay-cli/src/cli/commands/sandbox.rs`
- `crates/assay-core/src/mcp/proxy.rs`
- `crates/assay-evidence/src/trust_basis.rs`

`crates/assay-ebpf/src/vmlinux.rs` remains out of scope because it is generated kernel binding surface.

## Baseline

| Hotspot | Start LOC | Step | Target Shape |
| --- | ---: | --- | --- |
| `crates/assay-core/src/engine/runner.rs` | 696 | 1 | thin facade over `runner_next/*` |
| `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | 2 | `sandbox/*` command modules |
| `crates/assay-core/src/mcp/proxy.rs` | 813 | 3 | `proxy/*` config, transport, policy flow, event emission |
| `crates/assay-evidence/src/trust_basis.rs` | 2012 | 4 | `trust_basis/*` types, diff, generation, classifiers |

## Standing Rules

- Preserve public re-exports and command entrypoints.
- Keep mechanical move commits behavior-neutral.
- Add or keep contract tests before risky splits.
- Do not touch `.github/workflows/` in this wave.
- Do not change generated `vmlinux.rs`.
- Treat MCP and trust-basis JSON as protocol contracts, not internal implementation details.

## SOTA Gates

- `cargo fmt --check`
- `cargo check -p <crate>`
- targeted `cargo test` for the touched contract
- drift checks for facade thinness, forbidden workflow edits, and moved implementation boundaries
- later steps may add `cargo-semver-checks`, snapshot tests, or mutation smoke once the contracts are stable enough to make those signals meaningful

## Step Order

1. Finish `runner.rs` facade split by moving remaining implementation bodies into `runner_next`.
2. Split `sandbox.rs` into command orchestration, env filtering, tmp/profile handling, degradation payloads, and child execution helpers.
3. Characterize then split `mcp/proxy.rs`, keeping stdio/MCP protocol passthrough and decision emission stable.
4. Freeze canonical trust-basis output, then split `trust_basis.rs` into types, diff, generation, and classifier modules.

## Step 1 Status

Started. `runner.rs` now delegates assertion overlay and single-test execution to `runner_next::{assertions,single}` while keeping the same private method names for the existing runner execution path.
