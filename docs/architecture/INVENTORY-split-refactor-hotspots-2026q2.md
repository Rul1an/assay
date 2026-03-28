# Inventory: Split / Refactor Hotspots (Q2 2026)

> Status: Refreshed inventory baseline
> Date: 2026-03-27
> Baseline: `origin/main` @ `bfbb2989`
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
score, but it does help explain why some files are better next-wave candidates than others after
Wave43-Wave49 already retired the earlier MCP / registry / pack / sim hotspots.

| File | LOC | `fn` count | Test attrs | `unwrap/expect` | `unsafe` |
|---|---:|---:|---:|---:|---:|
| `crates/assay-core/tests/decision_emit_invariant.rs` | 1293 | 43 | 30 | 36 | 0 |
| `crates/assay-core/src/mcp/tool_call_handler/tests.rs` | 1242 | 38 | 27 | 3 | 0 |
| `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | 16 | 6 | 4 | 2 |
| `crates/assay-adapter-a2a/src/adapter_impl/tests.rs` | 729 | 28 | 23 | 37 | 0 |
| `crates/assay-core/src/engine/runner.rs` | 696 | 25 | 6 | 12 | 0 |
| `crates/assay-registry/src/auth.rs` | 685 | 27 | 13 | 11 | 0 |
| `crates/assay-evidence/src/trust_basis.rs` | 678 | 22 | 11 | 19 | 0 |
| `crates/assay-core/src/mcp/proxy.rs` | 672 | 21 | 12 | 9 | 0 |
| `crates/assay-core/src/storage/store.rs` | 658 | 34 | 0 | 23 | 0 |
| `crates/assay-core/src/vcr/mod.rs` | 654 | 26 | 7 | 4 | 0 |

## Most recent top 10 handwritten hotspots

This is the current raw size-ranked handwritten top 10 on `origin/main`, excluding the generated
`crates/assay-ebpf/src/vmlinux.rs` outlier.

| Rank | File | LOC | Kind |
|---|---|---:|---|
| 1 | `crates/assay-core/tests/decision_emit_invariant.rs` | 1293 | test-heavy |
| 2 | `crates/assay-core/src/mcp/tool_call_handler/tests.rs` | 1242 | test-heavy |
| 3 | `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | production |
| 4 | `crates/assay-adapter-a2a/src/adapter_impl/tests.rs` | 729 | test-heavy |
| 5 | `crates/assay-core/src/engine/runner.rs` | 696 | partially split production |
| 6 | `crates/assay-registry/src/auth.rs` | 685 | production |
| 7 | `crates/assay-evidence/src/trust_basis.rs` | 678 | fresh public surface |
| 8 | `crates/assay-core/src/mcp/proxy.rs` | 672 | production |
| 9 | `crates/assay-core/src/storage/store.rs` | 658 | partially split production |
| 10 | `crates/assay-core/src/vcr/mod.rs` | 654 | production |

## Inventory table

| Priority band | File | LOC | Role | Current split state | Immediate split posture | Suggested next wave |
|---|---|---:|---|---|---|---|
| `P1` | `crates/assay-registry/src/auth.rs` | 685 | Registry auth, token providers, OIDC exchange, auth caching | **Single security-sensitive module** with provider logic, token exchange, and cache coupling co-located | **Split next**. Best new production hotspot after Wave49: large enough, unsplit, and bounded around token provider / OIDC / exchange / cache seams | `R50` |
| `P1` | `crates/assay-core/src/mcp/proxy.rs` | 672 | MCP proxy config, lifecycle, process/http bridging, and test-heavy companion logic | **Still one dense module** despite being runtime-central | **Split soon after `R50`**. Good candidate for config, launch/lifecycle, and transport-facing helper seams behind a stable proxy surface | `R51` |
| `P1` | `crates/assay-cli/src/cli/commands/sandbox.rs` | 779 | Sandbox CLI orchestration, backend selection, degradation signaling, profile hookup | **Single command module** with embedded tests and `unsafe` usage | **Split soon, but after `R50/R51`**. Valuable, though a bit less central than registry/core runtime seams | `R52` |
| `P2` | `crates/assay-core/src/vcr/mod.rs` | 654 | VCR cassette client, scrub config, metadata, and response handling | **Single dense module** | **Strong later candidate** once the next registry/core hotspot is retired | `R53` |
| `P2` | `crates/assay-registry/src/lockfile.rs` | 649 | Registry lockfile read/write, materialization, and validation paths | **Single registry implementation module** | **Split later**. Reasonable next registry slice once auth has a stable freeze and split history behind it | `R54` |
| `P2` | `crates/assay-cli/src/cli/commands/profile.rs` | 651 | Profile CLI orchestration and status/report handling | **Single command module** | **Split later**. Similar shape to `sandbox.rs` but lower urgency | `R55` |
| `P3` | `crates/assay-core/tests/decision_emit_invariant.rs` | 1293 | Decision invariant regression tests | **Large test-only file** | **Do not split first as production refactor**. Treat as companion cleanup after the production seams it validates are fully stable | `T-R1` |
| `P3` | `crates/assay-core/src/mcp/tool_call_handler/tests.rs` | 1242 | Tool-call handler regression tests | **Large test-only file** | **Do not split first as production refactor**. Best handled only after the handler seams have had time to settle on `main` | `T-R2` |
| `P3` | `crates/assay-adapter-a2a/src/adapter_impl/tests.rs` | 729 | Adapter integration / contract regression tests | **Large test-only file** | **Defer** until there is an actual adapter production seam to decompose behind it | `T-A2A` |
| `Watchlist` | `crates/assay-core/src/engine/runner.rs` | 696 | Runner facade for eval execution | **Already partially split** via `runner_next/` | **Do not prioritize now**. Monitor facade thinness only; not a first refactor wave | monitor |
| `Watchlist` | `crates/assay-evidence/src/trust_basis.rs` | 678 | Trust Basis compiler core | **Fresh public trust-compiler surface** | **Do not split now**. Keep stable until the surrounding trust-compiler and release surface have had more soak time | monitor |
| `Watchlist` | `crates/assay-core/src/storage/store.rs` | 658 | Storage facade over `store_internal::*` helpers | **Already partially split** via `store_internal` | **Do not prioritize now**. Reassess only if the facade thickens again or store-internal boundaries become hard to review | monitor |

## Why these bands

### `P1` — best near-term split candidates

These are the strongest current candidates because they combine:

- large enough size to hurt reviewability
- central runtime or registry/CLI behavior
- bounded seams that can be moved mechanically behind stable facades
- immediate payoff without reopening freshly settled trust-compiler surfaces

### `P2` — valuable, but not the first wave

These are real hotspots, but the timing matters:

- [`crates/assay-core/src/vcr/mod.rs`](../../crates/assay-core/src/vcr/mod.rs) and
  [`crates/assay-registry/src/lockfile.rs`](../../crates/assay-registry/src/lockfile.rs) are both solid candidates, but they are slightly less urgent than the next registry auth / core proxy seams
- [`crates/assay-cli/src/cli/commands/profile.rs`](../../crates/assay-cli/src/cli/commands/profile.rs) is useful cleanup, but its payoff is lower than `sandbox.rs`

### `P3` — big files, but mostly test/support debt

The test-heavy files are worth splitting, but not as initial production waves:

- they should follow the production seam split they validate
- otherwise the repo risks spending an early wave on test reorganization without improving the underlying hotspot

## Recommended wave order

### `R50` — Registry auth split

Primary file:

- [`crates/assay-registry/src/auth.rs`](../../crates/assay-registry/src/auth.rs)

Goal:

- reduce `auth.rs` to a stable facade and isolate provider selection, OIDC exchange, cache
  handling, and request/header plumbing

Suggested target structure:

```text
crates/assay-registry/src/auth_next/
  mod.rs
  providers.rs
  oidc.rs
  cache.rs
  headers.rs
  diagnostics.rs
```

Hard constraints:

- no registry auth semantic drift
- no token refresh or cache lifetime drift
- no request/exchange URL drift
- no resolver or trust-store coupling drift

### `R51` — MCP proxy split

Primary file:

- [`crates/assay-core/src/mcp/proxy.rs`](../../crates/assay-core/src/mcp/proxy.rs)

Goal:

- isolate config parsing, process/http lifecycle, and proxy surface helpers behind the stable
  `McpProxy` entrypoint

Suggested target structure:

```text
crates/assay-core/src/mcp/proxy_next/
  mod.rs
  config.rs
  lifecycle.rs
  transport.rs
  diagnostics.rs
```

Hard constraints:

- no proxy runtime behavior drift
- no configuration-shape drift
- no request/response contract drift
- no coupling drift into handler/evidence/CLI layers

### `R52` — Sandbox command split

Primary file:

- [`crates/assay-cli/src/cli/commands/sandbox.rs`](../../crates/assay-cli/src/cli/commands/sandbox.rs)

Goal:

- keep CLI command UX stable while isolating backend detection, degradation payloads, profile wiring, and process execution

Why after `R50/R51`:

- it is important, but it is less central than the next registry/core runtime candidates
- it carries `unsafe` and backend-specific behavior, so the best split will benefit from the now-mature freeze/split/closure pattern

### `R53` / `R54` — VCR and registry lockfile follow-ons

Primary files:

- [`crates/assay-core/src/vcr/mod.rs`](../../crates/assay-core/src/vcr/mod.rs)
- [`crates/assay-registry/src/lockfile.rs`](../../crates/assay-registry/src/lockfile.rs)

Rule:

- choose between them after `R50-R52` based on whichever subsystem is creating more review pain
- both are better later than the auth/proxy/sandbox trio

### `T-R1` / `T-R2` / `T-A2A` — Follow-on test decomposition

Follow-on files:

- [`crates/assay-core/tests/decision_emit_invariant/main.rs`](../../crates/assay-core/tests/decision_emit_invariant/main.rs)
- [`crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs`](../../crates/assay-core/src/mcp/tool_call_handler/tests/mod.rs)
- [`crates/assay-adapter-a2a/src/adapter_impl/tests.rs`](../../crates/assay-adapter-a2a/src/adapter_impl/tests.rs)

Rule:

- only split these after the corresponding production seam is frozen
- otherwise the test refactor risks obscuring the real production split

## What should not be first

- generated files such as `crates/assay-ebpf/src/vmlinux.rs`
- fresh trust-compiler product surfaces such as [`crates/assay-evidence/src/trust_basis.rs`](../../crates/assay-evidence/src/trust_basis.rs)
- already partially split facades such as [`crates/assay-core/src/engine/runner.rs`](../../crates/assay-core/src/engine/runner.rs) and [`crates/assay-core/src/storage/store.rs`](../../crates/assay-core/src/storage/store.rs)
- test-only giant files before the next production seam is chosen
- simulation/support modules unless they are explicitly blocking product work

## Suggested next concrete step

If the repo wants one new refactor lane now, the best sequence is:

1. `R50` — `crates/assay-registry/src/auth.rs`
2. `R51` — `crates/assay-core/src/mcp/proxy.rs`
3. `R52` — `crates/assay-cli/src/cli/commands/sandbox.rs`
4. `R53/R54` — `vcr/mod.rs` vs `registry/lockfile.rs`

That order keeps the next unsplit production hotspots ahead of the giant test-only files and
avoids reopening newly stabilized trust-compiler surfaces too early.
