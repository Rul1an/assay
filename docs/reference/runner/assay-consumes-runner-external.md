# Assay Consumes Runner As External — Design Note

> Internal Phase 2D Slice 6A planning note. This page is design-only.
> It does **not** introduce code, redirect any imports, change any v0
> contract, or modify the boundary-map ownership table. Its purpose
> is to settle the design questions that block Slice 6B (the
> implementation slice that resolves extraction blocker #4).
>
> Historical status: this note pre-dates the post-Slice-6B cleanup that
> removed `assay-runner-spike` from the workspace. References to the
> wrapper crate below are retained as design-history context.

Slice 6 of the
[Assay-Runner extraction roadmap](extraction-roadmap.md) asks one
question:

> Can Assay consume the Runner candidate using only its public API,
> as if the Runner already lived in a separate repository?

When the answer is yes, blocker #4 (no non-spike external consumer
of the runner bundle format) is resolved and the runner boundary is
demonstrated rather than asserted. This note settles the design
choices that block the implementation PR.

## What Slice 6 is not

- not a code refactor of the runner crates themselves
- not a move of the composition that lives in
  `crates/assay-cli/src/cli/commands/runner_spike.rs` (that
  composition is the Assay-specific CLI orchestration; per the
  Slice 4 boundary-freeze it stays in `assay-cli` until a public
  runner entrypoint emerges)
- not a redesign of `assay-runner-schema`, `assay-runner-core`, or
  `assay-runner-linux`
- not a deletion or shrink of the runner crates' public API
- not a v0 contract change

## Current state — concrete inventory

After Slices 1-5B, the Runner candidate is split across three
publish-disabled crates and one thin compatibility wrapper:

| Crate | Hosts | Slice |
|---|---|---|
| `assay-runner-schema` | v0 data structures and schema constants | Slice 1 |
| `assay-runner-core` | runner orchestration, archive assembly, layer normalizers | Slice 2 |
| `assay-runner-linux` | Linux platform primitives (cgroup placement) | Slice 3 |
| `assay-runner-spike` | thin `pub use` re-export wrapper around the three crates above | pre-Slice 6 |

The only consumer of `assay-runner-spike` in the workspace today is
`crates/assay-cli/src/cli/commands/runner_spike.rs`. The full list
of symbols that file imports through the spike wrapper:

| Symbol | Actual home |
|---|---|
| `RunSpec` | `assay-runner-core` |
| `KernelLayerBuilder` | `assay-runner-core` |
| `RunnerSpikeArchive` | `assay-runner-core` |
| `PolicyLayerCapture` | `assay-runner-core` |
| `SdkLayerCapture` | `assay-runner-core` |
| `SDK_EVENT_SCHEMA` | `assay-runner-schema` |
| `CgroupCorrelationStatus` | `assay-runner-schema` |

Cgroup placement (`CgroupManager`, `SessionCgroup` from
`assay-runner-linux`) already bypasses the spike wrapper since
Slice 3.

Seven symbols, all already reachable from the three lower-level
crates directly. No symbol in the assay-cli import set requires
the spike wrapper for any reason.

## Design Decision A — Public Runner API surface

**Decision.** Assay consumes the Runner candidate through three
public crates, in this dependency-direction:

```
assay-cli
  ├── assay-runner-schema   (data types, schema strings, path consts)
  ├── assay-runner-core     (orchestration types, archive assembly, normalizers)
  └── assay-runner-linux    (cgroup placement — Linux platform adapter)
```

No new façade type, no new "MeasuredRun" entrypoint, no shared
prelude. The composition that wires these crates together lives in
the consumer (assay-cli's `runner_spike.rs` command). External
consumers are expected to write their own composition if they need
one; this is consistent with the
[Slice 4 platform-boundary freeze](extraction-roadmap.md#slice-4-platform-composition-boundary-landed-re-scoped)
which left composition in the consumer rather than promoting it
into core.

**Why no façade.** A "MeasuredRun" or "RunnerSession" façade in
`assay-runner-core` would require core to grow a platform
abstraction trait (today none — composition lives in assay-cli).
That re-opens the Slice 4 deferred-trait decision. The three
deferral triggers from Slice 4 have not fired (no second platform
spike, no core platform call site, no external consumer requiring
non-CLI composition). Without a triggering reason, premature
abstraction here would freeze a shape that may not match the
eventual real second consumer.

**What this means for external consumers.** Anyone consuming the
Runner candidate writes the same kind of composition that
`crates/assay-cli/src/cli/commands/runner_spike.rs` writes today: import
types from schema + core + linux, construct a `RunSpec`, manage
cgroup placement through `CgroupManager`/`SessionCgroup`, drive the
process, and assemble the archive via `RunnerSpikeArchive`. The
runner crates expose the typed building blocks; the consumer owns
the orchestration glue.

If a real second consumer arrives with reasons the typed building
blocks are insufficient (e.g. needs a non-CLI process model, or a
non-Linux platform), that becomes a separate slice that re-opens
the trait question. v0 stays narrow.

## Design Decision B — Forbidden imports

**Decision.** After Slice 6B, no production code outside the
`assay-runner-*` crate cluster imports anything from
`assay-runner-spike`. The spike wrapper becomes an unused crate.

Two acceptable end states for the wrapper itself:

1. **Keep as legacy alias.** `assay-runner-spike` stays in the
   workspace with its current `pub use` re-export surface but is
   not depended on by any other crate. It exists only as a
   navigational alias for readers reading older commits or
   external code that may still reference the name. `publish =
   false` continues; the crate is not consumed.
2. **Delete the wrapper entirely.** `crates/assay-runner-spike/` is
   removed from the workspace. `assay-cli`'s Cargo.toml drops the
   `assay-runner-spike` dependency.

Slice 6B picks between these two at implementation time. The
default proposal is (1), keep-as-legacy-alias, because deletion is
a one-way door and the cost of leaving a small unused crate is
minimal. (2) becomes the obvious move only if leaving the unused
crate creates a maintenance burden (e.g. clippy noise, dep-update
churn).

**The hard rule.** Slice 6B MUST achieve at minimum:

- `grep -r "assay_runner_spike::" crates/assay-cli/` returns no
  matches
- `crates/assay-cli/Cargo.toml` does not list `assay-runner-spike`
  as a dependency
- `cargo tree -p assay-cli` does not include `assay-runner-spike`

These are the mechanical evidence that Assay no longer hangs on
spike internals.

## Design Decision C — Smoke test

**Decision.** Slice 6B introduces **one new lane-check self-test
scenario**; the existing CI `cargo build -p assay-cli` remains the
compile proof. No new workflow, no new gate, no new delegated
proof requirement beyond what the existing lane-check already
enforces, and no new build-flag plumbing.

The one new check Slice 6B adds:

- **Mechanical absence check** in
  `scripts/ci/assay_runner_lane_check.py`'s `--self-test`. The
  scenario scans `crates/assay-cli/` for residual
  `assay_runner_spike::` references and
  `crates/assay-cli/Cargo.toml` for the `assay-runner-spike`
  dependency. Either appearing fails the self-test with a clear
  "Assay still consumes spike internals" message.

The existing CI surface stays the compile proof:

- The existing CI workflow already runs `cargo build -p assay-cli`
  on every Runner-impacting PR (including this slice category)
  because the lane-check classifier routes it that way. If
  `assay-cli` no longer depends on the spike crate, that build
  succeeding IS the proof of external-style consumption. No
  additional flag, feature, or `cargo build` invocation is
  introduced by Slice 6B.

The combination — absence check enforces the discipline; existing
build proves it compiles — covers both directions without adding
new CI surface.

**Why no new workflow.** The existing Runner CI lane already
classifies any change to `crates/assay-runner-*` or
`crates/assay-cli/` paths under runner discipline. Adding a
separate "external consumer smoke" workflow would duplicate the
existing surface without adding signal. The lane-check self-test
case carries the discipline forward at the right altitude.

**Why no new delegated proof.** The build-and-import test is a
local-CI mechanical check, not a runtime evidence claim. Delegated
proof remains required for any change that affects the runner
runtime path; the import-discipline check is orthogonal.

## Design Decision D — What stays in assay-cli

**Decision.** `crates/assay-cli/src/cli/commands/runner_spike.rs`
stays where it is, unchanged in behaviour, until at least one of
the Slice 4 re-open triggers fires:

1. A second platform spike (macOS or Windows) opens.
2. `assay-runner-core` itself gains a platform-abstraction call
   site.
3. An external consumer of `assay-runner-core` requires a non-CLI
   composition path.

The composition in `runner_spike.rs` is Assay-specific
orchestration glue: it wires `RunSpec` from
`assay-runner-core`, cgroup placement from `assay-runner-linux`,
event capture from `assay-monitor`, and outputs through
`assay-runner-core::RunnerSpikeArchive`. It is not a Runner public
entrypoint and would not naturally live in any of the runner
crates. Promoting it into core today would require either:

- introducing the `PlatformAdapter` trait that Slice 4 explicitly
  deferred, or
- inlining Assay's tokio + std::process composition into the
  runner crate, which would pull Assay-specific runtime choices
  into the Runner extraction candidate

Both options re-open settled decisions without a triggering
reason. The composition stays where it is.

What Slice 6B DOES change in `runner_spike.rs`:

- the import statements at the top: `use assay_runner_spike::{...}`
  → `use assay_runner_{schema,core}::{...}`
- the function-argument type annotations that mention
  `assay_runner_spike::X` → `assay_runner_{schema,core}::X`

No behaviour change. No public API surface change for the CLI
command itself.

## Design Decision E — Kill criteria

Slice 6B does not open if any of the following are true at the
time of opening:

1. **A second consumer arrives with conflicting requirements.** If
   someone outside Assay starts depending on `assay-runner-core`
   and reports that the three-crate import surface is too
   low-level, Slice 6B's "no façade" decision needs revisiting
   before any code lands.

2. **Schema, core, or linux fails Slice 6A's import inventory.**
   If, when implementing Slice 6B, it turns out that the seven
   symbols listed in the Current State section above are not
   sufficient to compile `assay-cli` without the spike wrapper —
   i.e. the spike wrapper exposes something Assay genuinely needs
   that doesn't have a direct schema/core/linux home — Slice 6B
   pauses and a separate slice promotes that symbol into the
   right crate first.

3. **The composition in `runner_spike.rs` itself starts importing
   from spike internals during the refactor.** The cutover must be
   purely a redirection of import paths; if it requires touching
   composition logic, the scope changed and the slice needs
   re-evaluation.

4. **Two consolidation windows pass with Slice 6B not opening
   after Slice 6A lands.** That suggests the design note missed
   something material; rather than letting Slice 6B drift, the
   roadmap revisits.

If killed, the kill is documented in this file (not silently
edited) and the extraction-readiness checkpoint records that
blocker #4 remains unresolved.

## Cutover plan for Slice 6B

Slice 6B is a single-PR, mechanical, runner-impacting change:

1. Add the lane-check self-test case described in Decision C.
   Verify it fails on the current main (because spike imports
   exist).
2. Rewrite the 11 occurrences of `assay_runner_spike::` in
   `crates/assay-cli/src/cli/commands/runner_spike.rs` to use
   `assay_runner_schema::` or `assay_runner_core::` directly,
   matching the symbol table in the Current State section. No
   behaviour change.
3. Remove `assay-runner-spike` from `crates/assay-cli/Cargo.toml`.
4. Verify the lane-check self-test now passes.
5. Decide spike end-state (keep-as-legacy-alias or delete) per
   Decision B. Document the choice in the PR body.
6. Update the boundary-map ownership row for `assay-runner-spike`
   to reflect the new state.
7. Mark Slice 6 as `✅ LANDED` in `extraction-roadmap.md` with
   resolution of blocker #4.
8. Mark blocker #4 as resolved in
   `platform-and-extraction-readiness.md`.
9. Delegated `gates=all` proof (Cargo.toml change → Gate.ALL).

Estimated scope: one Rust file with ~11 import-line edits, one
Cargo.toml dependency removal, optionally one crate deletion, plus
the standard docs and lane-check self-test updates. Smaller in
mechanical impact than Slices 5A or 5B; larger in extraction-
readiness signal because it resolves the last named structural
blocker.

## What this design note does NOT decide

- the spike wrapper's final fate (keep vs delete) — Slice 6B chooses
- the public crates' API surface beyond what Assay currently
  imports — future external consumers may demand more
- whether `crates/assay-cli/src/cli/commands/runner_spike.rs` ever moves
  out of `crates/assay-cli/` (Slice 4 deferred-trait territory)
- macOS or Windows platform work — separate readiness checkpoint
- a `PlatformAdapter` trait — Slice 4 deferred, triggers unchanged
- a public runner entrypoint or façade — Slice 4 deferred
- repository extraction (Slice 7) — gated behind the
  consolidation window after Slice 6B and the extraction blockers
- a third runtime fixture under `runner-fixtures/`

## Non-claims

- this note does not claim Slice 6B is small in review surface; the
  mechanical changes are limited but the discipline implications
  are not
- this note does not promise external consumers will adopt the
  three-crate import surface; it claims only that Assay itself can
- this note does not promise the spike crate stays alive; Slice 6B
  may delete it
- this note does not modify any v0 contract, golden, or fixture

## References

- [Assay-Runner extraction roadmap](extraction-roadmap.md)
- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Runner platform and extraction readiness](platform-and-extraction-readiness.md)
- Slice 4 (boundary-freeze): [`extraction-roadmap.md` § Slice 4](extraction-roadmap.md#slice-4-platform-composition-boundary-landed-re-scoped)
- Slice 5A (Gemini fixture move): #1322 (merged)
- Slice 5B (OpenAI Agents move + rename): #1323 (merged)
- Phase 1 acceptance: [`ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md`](../../notes/ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md)
