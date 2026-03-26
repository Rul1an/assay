# Inventory: Split / Refactor Hotspots (Q2 2026)

> Status: Proposed inventory baseline
> Date: 2026-03-26
> Baseline: `origin/main` @ `47b67769`
> Scope: Largest handwritten Rust hotspots plus immediate watchlist candidates
> Constraint: Inventory only; no behavior or API changes are implied by this document

## Why this inventory exists

The trust-compiler line is now on `main`, and the next refactor work should be driven by
explicit hotspot inventory rather than ad hoc file-size intuition.

This inventory is therefore intentionally narrow:

- focus on **handwritten Rust hotspots**
- distinguish **production**, **test-heavy**, and **already-partially-split** modules
- recommend a **wave order** that keeps behavior freeze and reviewability realistic
- avoid treating generated files or fresh trust-compiler surfaces as automatic split targets

## Baseline method

The baseline starts from the current largest handwritten Rust files on `origin/main`.

- Generated outliers are excluded from split priority:
  - `crates/assay-ebpf/src/vmlinux.rs`
- The initial production threshold is the current repo helper convention:
  - [`scripts/largest_rust_files.sh`](../../scripts/largest_rust_files.sh)
- A few sub-800 LOC watchlist files are included where they remain architecturally important
  even if they no longer cross the old threshold

## Hotspot signal snapshot

This is a compact snapshot of simple review signals at the baseline head. It is not a quality
score, but it does help explain why some files are better first-wave candidates than others.

| File | LOC | `fn` count | Test attrs | `unwrap/expect` | `unsafe` |
|---|---:|---:|---:|---:|---:|
| `crates/assay-core/src/mcp/decision.rs` | 1426 | 42 | 10 | 10 | 0 |
| `crates/assay-core/tests/decision_emit_invariant.rs` | 1293 | 43 | 30 | 36 | 0 |
| `crates/assay-core/src/mcp/tool_call_handler/tests.rs` | 1242 | 38 | 27 | 3 | 0 |
| `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` | 1016 | 31 | 0 | 0 | 0 |
| `crates/assay-sim/src/attacks/memory_poison.rs` | 954 | 27 | 9 | 1 | 0 |
| `crates/assay-evidence/src/lint/packs/schema.rs` | 844 | 22 | 10 | 1 | 0 |
| `crates/assay-registry/src/trust.rs` | 838 | 16 | 19 | 24 | 0 |
| `crates/assay-core/src/mcp/policy/engine.rs` | 799 | 25 | 3 | 2 | 0 |
| `crates/assay-evidence/src/lint/packs/checks.rs` | 785 | 27 | 3 | 3 | 0 |
| `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | 15 | 6 | 4 | 2 |

## Inventory table

| Priority band | File | LOC | Role | Current split state | Immediate split posture | Suggested next wave |
|---|---|---:|---|---|---|---|
| `P1` | `crates/assay-core/src/mcp/decision.rs` | 1426 | MCP decision kernel, event emission, convergence, replay basis | **Partially split already** via internal submodules (`consumer_contract`, `context_contract`, `deny_convergence`, `outcome_convergence`, `replay_compat`, `replay_diff`) but still one very large facade/body | **Split now**. Best next candidate for a thin facade + internal `decision_next/` style structure around emitter/guard, data projection, reason codes, serialization, and replay surfaces | `R1` |
| `P1` | `crates/assay-core/src/mcp/tool_call_handler/evaluate.rs` | 1016 | Tool-call evaluation flow, fail-closed handling, approval / scope / redact enforcement | **Not yet structurally split**; one large evaluation body under the already-split `tool_call_handler/` directory | **Split now**. Good mechanical candidate for `approval`, `restrict_scope`, `redact_args`, `policy_eval`, and `result_mapping` seams behind stable handler types | `R2` |
| `P1` | `crates/assay-core/src/mcp/policy/engine.rs` | 799 | MCP policy engine decisions and metadata enrichment | **Still dense single module** with policy, deny/match, obligation, and metadata logic co-located | **Split soon after `R1/R2`**. Strongly coupled to decision/tool-call behavior, so it should follow once decision seams are frozen | `R3` |
| `P1` | `crates/assay-evidence/src/lint/packs/schema.rs` | 844 | Pack schema, serde shape, validation contract | **Single schema module** with pack metadata, rule types, conditional forms, validation errors | **Split now**. Good low-drift candidate into `types`, `serde`, `validation`, and `conditional` seams | `R4` |
| `P1` | `crates/assay-evidence/src/lint/packs/checks.rs` | 785 | Pack engine check execution | **Single implementation module** for many pack checks | **Split now or immediately after `schema.rs`**. Natural boundary around event checks, manifest checks, JSON path checks, and auth-context checks | `R4` |
| `P2` | `crates/assay-registry/src/trust.rs` | 838 | Trust store, pinned roots, manifest refresh, cache lifetime | **Single security-sensitive module** | **Split later, not first**. Worth doing, but only with strict behavior freeze because trust-store semantics and cache timing are easy to drift | `R5` |
| `P2` | `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | Sandbox CLI orchestration, backend selection, degradation signaling, profile hookup | **Single command module** with embedded tests | **Split later** into CLI orchestration vs backend/degradation/profile helpers. Medium payoff, but less urgent than core MCP / pack engine hotspots | `R6` |
| `P3` | `crates/assay-core/tests/decision_emit_invariant.rs` | 1293 | Decision invariant regression tests | **Large test-only file** | **Do not split first as production refactor**. Treat as companion cleanup after `R1`, likely into fixtures + scenario groups | `T-R1` |
| `P3` | `crates/assay-core/src/mcp/tool_call_handler/tests.rs` | 1242 | Tool call handler regression tests | **Large test-only file** | **Do not split first as production refactor**. Best handled as a follow-on to `R2`, once handler seams are stable | `T-R2` |
| `P3` | `crates/assay-sim/src/attacks/memory_poison.rs` | 954 | Deterministic attack simulation vectors | **Single simulation module** | **Defer**. Useful cleanup candidate, but not a current operational hotspot on the main trust-compiler/product path | `R7` |
| `Watchlist` | `crates/assay-core/src/engine/runner.rs` | 696 | Runner facade for eval execution | **Already partially split** via `runner_next/` | **Do not prioritize now**. Monitor facade thinness only; not a first refactor wave | monitor |
| `Watchlist` | `crates/assay-evidence/src/trust_basis.rs` | 678 | Trust Basis compiler core | **Fresh trust-compiler surface** | **Do not split now**. Keep stable while `T1/P2/G4` settle; avoid refactoring a newly-shipped public surface too early | monitor |

## Why these bands

### `P1` — best near-term split candidates

These are the strongest candidates because they combine:

- large enough size to hurt reviewability
- central runtime or pack-engine behavior
- bounded seams that can be moved mechanically behind stable facades
- immediate payoff for future trust-compiler and MCP work

### `P2` — valuable, but not the first wave

These are real hotspots, but the timing matters:

- [`crates/assay-registry/src/trust.rs`](../../crates/assay-registry/src/trust.rs) is security-sensitive and should not be mixed into a broader operational refactor wave
- [`crates/assay-cli/src/cli/commands/sandbox.rs`](../../crates/assay-cli/src/cli/commands/sandbox.rs) has `unsafe` and backend-specific behavior, but it is less central to the current trust-compiler lane than MCP and pack-engine hotspots

### `P3` — big files, but mostly test/support debt

The test-heavy files are worth splitting, but not as initial production waves:

- they should follow the production seam split they validate
- otherwise the repo risks spending an early wave on test reorganization without improving the underlying hotspot

## Recommended wave order

## `R1` — Decision kernel split

Primary file:

- [`crates/assay-core/src/mcp/decision.rs`](../../crates/assay-core/src/mcp/decision.rs)

Goal:

- reduce `decision.rs` to a thin facade
- isolate decision-event emission, guard lifecycle, projection, and replay-facing helpers

Suggested target structure:

```text
crates/assay-core/src/mcp/decision_next/
  mod.rs
  emitter.rs
  guard.rs
  event_types.rs
  decision_data.rs
  reason_codes.rs
  replay.rs
  tests.rs
```

Hard constraints:

- no decision contract drift
- no reason-code renames
- no event payload shape changes
- no replay-basis behavior changes

## `R2` — Tool-call evaluation split

Primary file:

- [`crates/assay-core/src/mcp/tool_call_handler/evaluate.rs`](../../crates/assay-core/src/mcp/tool_call_handler/evaluate.rs)

Goal:

- break the large evaluation flow into mechanical sub-seams behind the existing handler facade

Suggested target structure:

```text
crates/assay-core/src/mcp/tool_call_handler/evaluate_next/
  mod.rs
  policy_eval.rs
  approval.rs
  restrict_scope.rs
  redact_args.rs
  mandate.rs
  result_mapping.rs
```

Hard constraints:

- stable `ToolCallHandler` public surface
- no obligation outcome drift
- no fail-closed / degrade-read-only behavior drift
- no event emission ordering drift

## `R3` — Policy engine split

Primary file:

- [`crates/assay-core/src/mcp/policy/engine.rs`](../../crates/assay-core/src/mcp/policy/engine.rs)

Goal:

- separate matching, deny classification, obligation extraction, and metadata projection

Why after `R1/R2`:

- the current decision / handler / policy coupling is real
- splitting `engine.rs` first would make drift-hunting harder because the consumer seams above it are still large

## `R4` — Pack engine schema + checks split

Primary files:

- [`crates/assay-evidence/src/lint/packs/schema.rs`](../../crates/assay-evidence/src/lint/packs/schema.rs)
- [`crates/assay-evidence/src/lint/packs/checks.rs`](../../crates/assay-evidence/src/lint/packs/checks.rs)

Goal:

- make the pack engine easier to extend without another mega-file

Suggested sequence:

1. `schema.rs` first
2. `checks.rs` second

Reason:

- the schema/types side is a cleaner freeze point
- then the runtime check execution can split on a stable type boundary

## `R5` — Registry trust store split

Primary file:

- [`crates/assay-registry/src/trust.rs`](../../crates/assay-registry/src/trust.rs)

Goal:

- separate pinned roots, manifest refresh, cache/TTL handling, and key-metadata mutation

Risk note:

- this is a **security-sensitive** split candidate
- it should be its own wave with stronger contract tests than the average refactor slice

## `R6` — Sandbox command split

Primary file:

- [`crates/assay-cli/src/cli/commands/sandbox.rs`](../../crates/assay-cli/src/cli/commands/sandbox.rs)

Goal:

- keep CLI command UX stable while isolating backend detection, degradation payloads, profile wiring, and process execution

Why not earlier:

- it is important, but not the biggest current multiplier for trust-compiler or MCP iteration speed

## `T-R1` / `T-R2` — Follow-on test decomposition

Follow-on files:

- [`crates/assay-core/tests/decision_emit_invariant.rs`](../../crates/assay-core/tests/decision_emit_invariant.rs)
- [`crates/assay-core/src/mcp/tool_call_handler/tests.rs`](../../crates/assay-core/src/mcp/tool_call_handler/tests.rs)

Rule:

- only split these after the corresponding production seam is frozen
- otherwise the test refactor risks obscuring the real production split

## What should not be first

- generated files such as `crates/assay-ebpf/src/vmlinux.rs`
- fresh trust-compiler product surfaces such as [`crates/assay-evidence/src/trust_basis.rs`](../../crates/assay-evidence/src/trust_basis.rs)
- already partially split facades such as [`crates/assay-core/src/engine/runner.rs`](../../crates/assay-core/src/engine/runner.rs)
- simulation/support modules unless they are explicitly blocking current product work

## Suggested next concrete step

If the repo wants one new refactor lane now, the best sequence is:

1. `R1` — `mcp/decision.rs`
2. `R2` — `mcp/tool_call_handler/evaluate.rs`
3. `R3` — `mcp/policy/engine.rs`
4. `R4` — pack engine (`schema.rs` then `checks.rs`)

That order keeps the highest-value runtime hotspots ahead of the secondary CLI / registry / simulation cleanup line.
