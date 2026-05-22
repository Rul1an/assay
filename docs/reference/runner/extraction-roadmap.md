# Assay-Runner Extraction Roadmap

> Internal Phase 2D planning note. This page defines the forward-looking
> slice sequence that moves the Assay-Runner candidate toward
> extraction-readiness. It does NOT extract Assay-Runner, does NOT open a
> new repository, does NOT propose a schedule, and does NOT supersede the
> authoritative ownership table in
> [`boundary-map.md`](boundary-map.md) or the current verdict in
> [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md).
> It complements those two documents with a concrete slice ordering and a
> per-PR discipline rule that applies from now on.

This page exists because two questions stayed open after Phase 2C:

1. The boundary-map names *what* must move; it does not name *what comes
   first*. Out-of-order moves create dependency cycles between slices.
2. The readiness checkpoint names *triggers* for reopening extraction;
   it does not name *the slices that produce those triggers*.

The roadmap closes those two gaps without re-opening any settled
contract.

## Load-Bearing Claims

Three claims define how this roadmap is to be read:

1. **Extraction is not the goal of any individual PR.** Extraction
   readiness IS the goal of every Runner-impacting PR from now on.
2. **Slice order is load-bearing.** Schemas before core, core before
   platform, platform before fixture extraction, fixture extraction
   before in-monorepo external-consumer test, external-consumer test
   before any repository split discussion. Reordering breaks
   dependencies.
3. **Names are committed, scope is committed.** The first concrete code
   slice is `crates/assay-runner-schema` with the exact scope below.
   This is not "we will think about it"; it is the next discoverable
   PR.

## What this document is not

- Not the authoritative boundary table; that stays in
  [`boundary-map.md` § Boundary Table](boundary-map.md#boundary-table).
- Not the authoritative extraction criteria checklist; that stays in
  [`boundary-map.md` § Extraction Readiness Criteria](boundary-map.md#extraction-readiness-criteria).
- Not the current verdict on extraction; that stays in
  [`platform-and-extraction-readiness.md` § Extraction Readiness](platform-and-extraction-readiness.md#extraction-readiness).
- Not a schedule, calendar window, or quarter commitment.
- Not a repository name, layout, ownership, or publication decision.
- Not a macOS or Windows planning slice.

If a future PR needs to relitigate any of the above, the answer is in
those other documents, not here.

## Per-PR Discipline Rule

From now on, every **Runner-impacting** PR MUST include a short
"Extraction boundary impact" section in its body that takes exactly
one of three shapes.

A PR is *Runner-impacting* when it changes any of:

- `crates/assay-runner-spike/` (or any future `crates/assay-runner-*/`)
- `crates/assay-monitor/` or `crates/assay-ebpf/` (Assay-owned runner
  substrate that the boundary table calls out)
- `crates/assay-cli/src/cgroup.rs` or `crates/assay-cli/src/cli/commands/runner_spike.rs`
- `scripts/ci/runner-spike-*.sh` or
  `scripts/ci/assay_runner_*_validate.py`
- `tests/fixtures/runner-spike/`
- `.github/workflows/runner-spike-delegated.yml` or
  `.github/workflows/runner-spike-sdk.yml`
- `docs/reference/runner/**` (recursive — this includes the
  contracts (`*.md`), the boundary map, this roadmap, the v0 goldens
  under `docs/reference/runner/golden/*.json`, the v0 JSON Schemas
  under `docs/reference/runner/schema/*.json`, and any future
  subdirectory under `docs/reference/runner/`)

This is broader than the lane-check classifier's `Gate.NONE` boundary,
because lane-check decides whether a PR needs delegated proof and not
whether it changes extraction boundaries. Many Gate.NONE PRs
(e.g. edits to `docs/reference/runner/boundary-map.md` itself, or to a
runner schema sidecar) DO move extraction boundaries and therefore
fall under the discipline rule below. The two checks are
complementary, not overlapping.

The Extraction boundary impact section must take exactly one of
three shapes:

A. **Resolves a blocker.** Cite which structural blocker from
   [`platform-and-extraction-readiness.md` § Extraction Readiness](platform-and-extraction-readiness.md#extraction-readiness)
   the PR resolves, and how.

B. **Moves a boundary without resolving a blocker.** Cite which row of
   the boundary table the PR affects and how the change moves toward
   extraction-readiness rather than away from it.

C. **Does not change extraction boundaries.** Explicit statement.
   Reviewers verify this against the changed paths before merge.

PRs that mix moves toward extraction with moves away from it are not
allowed. Split them.

This rule applies regardless of whether the slice originated from this
roadmap or from another work line (e.g. `#1271` ring-buffer
diagnostics). The discipline is per-PR; the slice list below is
guidance for ordering net-new extraction work.

## The Four Current Structural Blockers

These are the four priority blockers named in
[`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md#extraction-readiness),
restated here only so each slice below can point at the blocker it
resolves.

| # | Blocker | Resolved by |
|---|---|---|
| 1 | `assay.runner.*.v0` schemas live in `crates/assay-runner-spike/src/` | Slice 1 (`assay-runner-schema`) |
| 2 | Archive verification crosses an unresolved boundary conflict in `crates/assay-runner-spike/src/archive.rs` | Slice 1 (manifest schema) + Slice 2 (assembly relocation) |
| 3 | Cgroup placement depends on `crates/assay-cli/src/cgroup.rs` | Slice 3 (cgroup API extraction) |
| 4 | No non-spike external consumer of the runner bundle format | Slice 6 (Assay-consumes-Runner-as-external) |

## Slice Sequence

Each slice is a separate PR. Each slice's PR body MUST cite this
roadmap by section and state which blocker it resolves (or the boundary
row it moves) per the Per-PR Discipline Rule above.

### Slice 1 — `crates/assay-runner-schema` ✅ LANDED

> Resolves blocker #1 fully; partial resolution of blocker #2 (manifest
> semantics half). The assembly half of blocker #2 moves in Slice 2.

**Scope.** Move data structures and constants for the `assay.runner.*.v0`
schemas out of `crates/assay-runner-spike/src/` into a new
publish-ready crate `crates/assay-runner-schema`.

Crate contents:

- `assay.runner.observation_health.v0` types (from
  `crates/assay-runner-spike/src/health.rs`)
- `assay.runner.capability_surface.v0` types (from
  `crates/assay-runner-spike/src/surface.rs`)
- `assay.runner.correlation_report.v0` types (from
  `crates/assay-runner-spike/src/correlation.rs`)
- `assay.runner.sdk_event.v0` types (currently inline in
  `crates/assay-runner-spike/src/sdk.rs`)
- Archive manifest schema constants and value types (currently inline
  in `crates/assay-runner-spike/src/archive.rs`; this is the manifest semantics
  half of the archive boundary conflict from
  [`boundary-map.md` § Active Boundary Conflicts](boundary-map.md#active-boundary-conflicts))
- Schema string constants and version constants

Crate excludes (must NOT land here):

- eBPF or monitor code
- CLI surface
- Fixture code
- Filesystem I/O for archive assembly (that is Slice 2)
- Capability-diff or cross-runtime-diff projection logic (those live
  in `scripts/ci/` today; whether they later move into this crate is
  a separate slice decision)

Compatibility:

- `assay-runner-spike` re-exports the moved types from
  `crates/assay-runner-schema` so existing call sites compile unchanged.
- The schema crate has zero internal dependencies on `assay-cli`,
  `assay-monitor`, or `assay-ebpf`.
- Schema crate publishability: `publish = false` until Slice 7; the
  crate is structured as if it could be published, but is not.

Resolves: blocker #1 fully; partial progress on blocker #2 (manifest
semantics half).

### Slice 2 — `crates/assay-runner-core` ✅ LANDED

> Resolves blocker #2 fully (archive boundary conflict completely
> resolved together with Slice 1's manifest semantics move).

**Scope.** Move runner orchestration and archive assembly into a new
`crates/assay-runner-core` crate that depends on
`crates/assay-runner-schema` for types.

Crate contents:

- `RunSpec` and execution orchestration (from
  `crates/assay-runner-spike/src/run.rs`)
- Archive assembly and writing (from
  `crates/assay-runner-spike/src/archive.rs`, *after* manifest schema moved
  to `assay-runner-schema` in Slice 1)
- Normalizers: kernel, policy, SDK (from `crates/assay-runner-spike/src/`
  using schema types)
- Re-export surface for current `assay-runner-spike` consumers

Allowed dependencies for `assay-runner-core`:

- `assay-runner-schema` (this slice introduces this dep)
- `assay-monitor`, `assay-ebpf`, `assay-common` (existing Assay-owned
  substrate; future-platform separation is Slice 4)
- standard runtime crates (`serde`, `serde_json`, etc.)

Forbidden dependencies:

- `assay-cli` (especially `crates/assay-cli/src/cgroup.rs`; this is
  Slice 3)
- `assay-evidence` direct usage where shared contract types would
  suffice

Compatibility:

- `assay-runner-spike` continues to compile as a thin re-export and
  test-fixture wrapper of `assay-runner-core`.
- Existing call sites in `assay-cli` continue to work; the cgroup
  dependency from `assay-cli` side is intentionally untouched in this
  slice (its turn is Slice 3).

Resolves: blocker #2 fully (archive assembly relocates without crossing
back into Assay artifact semantics; manifest semantics already moved
in Slice 1).

### Slice 3 — Cgroup API extraction ✅ LANDED

> Resolves blocker #3 fully. Executed via Option B (`crates/assay-runner-linux`
> introduced immediately) with an extremely narrow scope: only cgroup
> placement moved. eBPF monitor adapter remains in `assay-monitor`;
> macOS/Windows adapters are out of scope until their separate platform
> spikes open under `platform-and-extraction-readiness.md`.

**Scope.** Move `crates/assay-cli/src/cgroup.rs` to a stable cgroup
API surface in `assay-runner-linux` that `assay-cli` consumes
directly. `assay-runner-core` does not consume the cgroup API
because the placement call sites live in `assay-cli`'s
`runner_spike.rs` orchestration command, not inside core. The
exit invariant remains the same: no runner crate (current or
future-extracted) depends on `assay-cli` for cgroup placement.

Two execution paths are acceptable. Choose at slice-open time, not
here:

Option A: Move the cgroup module to `assay-runner-core` as a Linux
platform sub-module. Simpler. Defers platform separation to Slice 4.

Option B: Introduce `crates/assay-runner-linux` immediately (collapses
Slice 3 and Slice 4 into one). Cleaner platform line. Bigger PR.

The roadmap does not pick between A and B; whichever is chosen, the
exit invariant is the same: `assay-runner-core` (and any future
extracted runner crate) MUST NOT depend on any `assay-cli` module for
cgroup placement.

Resolves: blocker #3 fully.

### Slice 4 — Platform composition boundary ✅ LANDED (re-scoped)

> **Re-scope note.** The original Slice 4 text below proposed moving
> the eBPF monitor adapter into `crates/assay-runner-linux` and
> introducing a `PlatformAdapter` trait in `assay-runner-core`. After
> Slices 1-3 landed, that framing no longer matched the actual code
> shape:
>
> - `assay-monitor` is **shared Assay measurement substrate**: it is
>   consumed by the runner candidate AND by Assay's own
>   `assay monitor` standalone CLI command. Moving it into a runner
>   crate would break the non-runner consumer.
> - `assay-ebpf` is **Assay-owned Linux kernel program substrate**;
>   it is the kernel-side counterpart of the shared measurement
>   substrate.
> - `assay-runner-core` itself has no platform-specific call sites
>   today. The cgroup/process-spawn composition lives in
>   `assay-cli/src/cli/commands/runner_spike.rs`. A `PlatformAdapter`
>   trait in core would have no caller inside core, only in CLI.
>
> Slice 4 therefore landed as a **boundary-freeze docs slice**, not
> as a code-extraction slice. It explicitly confirms that
> `assay-monitor` and `assay-ebpf` stay Assay-owned, that
> `assay-runner-linux` remains placement-primitives-only, and that
> the runner composition currently lives in `assay-cli` as a
> temporary layer until Slice 6.

**Actual scope (boundary-freeze).**

1. **Boundary-map ownership lines made explicit.** Each of the four
   ownership rows below now states its role unambiguously:
   - `assay-monitor` → shared Assay measurement substrate (runner +
     standalone Assay command).
   - `assay-ebpf` → Assay-owned Linux kernel program substrate.
   - `assay-runner-linux` → runner Linux **placement primitives
     only**; no eBPF/monitor inside.
   - `assay-cli/src/cli/commands/runner_spike.rs` → temporary
     composition layer until Slice 6.
2. **`PlatformAdapter` trait deferred.** Not introduced. The trait
   has no current consumer inside `assay-runner-core` and no second
   implementation to validate it against. Premature abstraction risks
   freezing a shape that does not match the eventual macOS/Windows
   reality.
3. **No code move.** `assay-monitor` and `assay-ebpf` stay in their
   current crates. `assay-runner-linux` remains cgroup-only.
   `runner_spike.rs` composition helpers (pre-exec PID writers,
   retry helpers) stay in CLI because they are tightly coupled to
   the spawn flow that lives there.

**Re-open triggers (when to revisit the deferred trait).**

The `PlatformAdapter` trait is deferred until **any one** of:

1. A second platform spike (macOS or Windows) opens under
   [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md).
   The trait then has a real second implementation to validate against
   and is no longer speculative.
2. `assay-runner-core` itself gains a platform-abstraction call site
   (today it has none — the cgroup/spawn composition lives in
   `assay-cli`, not in core). If core acquires a platform-dependent
   surface, the trait gains a real caller.
3. An external consumer of `assay-runner-core` requires a non-CLI
   composition path. That would mean Slice 6 has opened a public
   runner entrypoint that bypasses `assay-cli/src/cli/commands/runner_spike.rs`
   and that entrypoint needs to express platform variability through
   a trait rather than a direct dependency.

Until at least one of those fires, the platform boundary stays as
described in the boundary-map: `assay-runner-linux` is the Linux
placement crate, `assay-monitor` is shared substrate, and the
composition lives in `assay-cli`.

**Resolves.** Structural clarity about what is and is not in the
runner platform crate. Does NOT resolve any of the four named
extraction blockers — blocker #3 was already resolved by Slice 3,
and #4 is Slice 6 territory. Slice 4's value is preventing
premature abstraction and documenting where future platform work
will plug in.

### Slice 5 — Fixture package boundary 🟡 IN PROGRESS

> Slice 5 splits into two sub-PRs to keep each fixture's move
> reviewable in isolation:
>
> - **Slice 5A — Gemini fixture move** ✅ LANDED. Gemini Python
>   google-genai fixture moved from
>   `tests/fixtures/runner-spike/gemini-google-genai/` to
>   `runner-fixtures/gemini-google-genai/`. The wrapper renamed from
>   `gemini-google-genai-sdk-policy-agent.sh` to
>   `runner-fixtures/gemini-google-genai/sdk-policy-agent.sh` (dropped
>   the runtime prefix because it now lives inside the runtime
>   package). Acceptance scripts, workflow install path, lane-check
>   classifier rules, and runner reference docs updated accordingly.
> - **Slice 5B — S5 OpenAI Agents fixture move + rename** ✅ LANDED.
>   `tests/fixtures/runner-spike/openai-agents-js/` moved to
>   `runner-fixtures/openai-agents/` and the wrapper
>   `tests/fixtures/runner-spike/openai-agents-sdk-policy-agent.sh`
>   moved to `runner-fixtures/openai-agents/sdk-policy-agent.sh`,
>   dropping the `-js` suffix because the fixture identity is the
>   runtime, not the implementation language. Slice 5B also renamed
>   the SDK source-identity string emitted by the fixture from
>   `openai-agents-js-fixture` to `openai-agents-fixture` so the
>   on-disk fixture identity, the package-boundary directory name,
>   and the recorded evidence stay consistent.

**Scope.** Move `tests/fixtures/runner-spike/` into a runner-owned
fixtures layout that is structured as if it were a separate package.

Probable shape:

- `runner-fixtures/openai-agents/`
- `runner-fixtures/gemini-google-genai/`
- Each fixture has its own dependency manifest (pip requirements,
  package-lock.json) and acceptance entrypoint

`scripts/ci/runner-spike-*.sh` continue to operate but reference
fixtures by their new path.

This slice has lower urgency than Slices 1-4 (it does not resolve a
named structural blocker), but it should land before Slice 6 so the
external-consumer test exercises real fixture packaging, not
test-tree-internal paths.

Resolves: cleaner separation when Slice 7 needs to split the repo.

### Slice 6 — Assay consumes Runner as external

**Scope.** Within the same monorepo, restructure dependencies so that
Assay CLI and Assay-Harness depend on the *public API* of
`assay-runner-core` (and `assay-runner-schema`), not on internal
modules or test-tree wrappers.

Specifically:

- All `assay-runner-spike` internal imports from outside the runner
  crate hierarchy are removed.
- A public API surface is documented (probably via the crate-level
  `lib.rs` doc comment plus a small API stability note).
- Assay's existing capability-diff and cross-runtime-diff validators
  (`scripts/ci/assay_runner_*_validate.py`) confirm that they operate
  on schema crate types only, not spike internals.
- A new in-monorepo smoke test asserts that
  `cargo build -p assay-runner-core` and
  `cargo build -p assay-runner-schema` work with the rest of the
  monorepo treating them as external dependencies (no internal-only
  feature flags, no path-only deps assumed available, etc.).

Resolves: blocker #4 (provides the missing non-spike external
consumer, satisfied by Assay itself in external-style consumption).

### Slice 7 — Repository split (gated)

**Scope.** Create a separate Assay-Runner repository and move the
runner crates, fixtures, delegated workflow templates, schemas,
golden examples, proof-pack format, and runner docs into it. Assay
keeps Harness, policy/trust-basis interpretation, higher-level CLI,
product docs, and the consumer side of runner evidence.

**Hard gates before this slice may open:**

1. All 15 [`Extraction Readiness Criteria`](boundary-map.md#extraction-readiness-criteria)
   in the boundary map are green.
2. Zero of the 11 [`Extraction Blocking Conditions`](boundary-map.md#extraction-blocking-conditions)
   are true.
3. Slices 1-6 have all landed and have passed at least one consolidation
   window (4-6 weeks) without semantic churn on the boundaries they
   touched.
4. A concrete external use case exists (per the trigger in
   [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md#extraction-readiness)).
   "We could split now" is not a trigger.

If any of the four gates is not green, this slice does not open. The
correct response is to open a sub-slice that closes the missing gate,
not to force the split.

This slice does not name the new repository, license, branch
protection, CI surface, or publication target. Those are PR-body
content of the slice itself when it opens, not of this roadmap.

## Cross-Slice Discipline

The following rules apply across the slice sequence, not to any
individual slice:

- **No silent schema changes.** Slices 1-2 may rename, restructure, or
  refactor schema types, but they may NOT change the v0 schema string
  values, the v0 field set, or the v0 value vocabulary. Schema
  semantics are frozen; only their hosting crate moves.
- **No silent acceptance loosening.** Every slice must preserve the
  existing delegated acceptance gates (`ringbuf_drops=0`, three-run
  determinism, `tool_call_id` correlation requirement).
- **No new platform support introduced inside a slice.** macOS and
  Windows remain governed by
  [`platform-and-extraction-readiness.md`](platform-and-extraction-readiness.md).
  Slice 4 makes a future platform spike possible; it does not start
  one.
- **No new contract opened by accident.** Slices 1-6 are pure
  refactoring + boundary moves; they do not introduce new v0 or v1
  contracts. If a contract change becomes necessary mid-slice, split
  the slice.

## Kill Criteria For The Roadmap Itself

This roadmap is itself revisable. It must be revisited (and either
revised or paused) if any of these become true:

1. A structural blocker cannot be resolved without breaking an existing
   acceptance gate. (For example: Slice 1 would have to weaken
   `assay.runner.observation_health.v0` to fit a shared crate.)
2. Two consolidation windows pass with Slice N landed but Slice N+1
   not opening, indicating the next-slice work is harder than the
   roadmap claims.
3. An external consumer concretely arrives with requirements that do
   not fit the boundary table in
   [`boundary-map.md`](boundary-map.md#boundary-table). The boundary
   table is then updated *first*, not this roadmap.
4. A second platform (macOS or Windows) becomes a real spike before
   Slice 4 lands. In that case, Slice 4 is rewritten to match the
   actual adapter requirements that emerge from the platform spike.

If killed, this roadmap is replaced by a successor note that explicitly
references this file. It is not silently edited.

## Non-Goals

This roadmap does not:

- choose a repository name
- choose a license
- propose a publication target
- choose between Option A and Option B in Slice 3
- propose macOS or Windows measurement
- modify any v0 contract
- modify any v0 golden
- modify any delegated workflow
- modify the lane-check classifier
- introduce new dependencies
- open a new issue
- pre-approve any extraction PR

## What This Roadmap Unlocks

After this roadmap lands, the next discoverable step is **Slice 1**:
the `crates/assay-runner-schema` crate as a separate PR, with its PR
body citing this roadmap and naming blocker #1 as the resolution
target.

No code, fixture, or workflow change opens Slice 1. That PR is the
opening. This document only sequences and disciplines.

## Phase 2D Visibility — Standalone Usefulness vs Repository Extraction

A common misreading of this roadmap is to treat "extraction" and
"standalone usefulness" as the same thing. They are not. This section
exists to make the distinction explicit before any future PR
conflates them.

### Where Assay-Runner can be useful on its own

Assay-Runner can be valuable to users who do not need
Assay-Harness, Trust Basis compilation, or the higher-level Assay
product surfaces. Plausible scenarios:

- **Own policy engine.** An organisation already has an internal
  policy / review layer and wants only reliable agent-run evidence:
  `observation-health`, `capability-surface`, `correlation-report`.
  Their own governance interprets acceptability.
- **Agent runtime benchmarking.** Comparing OpenAI Agents, Gemini,
  Anthropic, Vercel AI SDK, etc. on observed capabilities without
  Assay's acceptability semantics.
- **Runtime-security / research tooling.** Using eBPF + SDK/policy
  correlation as a measurement instrument without adopting the full
  Assay product layer.
- **CI attestation.** A CI platform produces a proof-pack artifact on
  agent runs so downstream systems can review later.
- **Regulated environments.** Evidence generation runs in a
  restricted environment while policy interpretation happens
  elsewhere.
- **Agent framework maintainers.** Adding a Runner fixture to make a
  Python/JS agent framework measurable, without learning Harness.

### Standalone usefulness is not the same as repository extraction

> **Kernzin.** Standalone usefulness is not the same as repository
> extraction. Phase 2D's first goal is to make Assay-Runner externally
> consumable *inside* the monorepo; a separate repository is only
> justified after a real external consumer exists.

Standalone usefulness is achieved by Slices 1-6 of this roadmap:

- own crates (Slices 1-4)
- own docs and schemas (already on `main`)
- own fixture package (Slice 5)
- own public API consumed externally even within the monorepo
  (Slice 6)
- own CI lane (already on `main` via the lane-check classifier)

That is roughly 80% of the standalone-product experience without the
governance, branch-protection, release-cycle, and CI surface costs of
a repository split.

### Why repository extraction is the wrong first goal

A premature repo split has five specific costs that the roadmap
chooses to avoid:

1. **Contract churn.** While `assay.runner.*.v0` schemas are still
   moving (Slice 1 moves their crate hosting), a separate repo makes
   every schema change cross-repository.
2. **Dual CI / governance.** Two PR streams, two release cycles,
   two branch-protection surfaces, two security-policy files.
3. **Boundary not yet proven.** Until Assay itself consumes Runner
   through a public API (Slice 6), publishing a boundary externally
   means publishing the wrong one.
4. **No external consumer.** Optimising for a hypothetical user is
   worse than optimising for the consumer who actually arrives.
5. **Evidence / Harness interaction is still informative.**
   Harness/Trust-Basis consumption is still surfacing useful pressure
   on Runner contracts. Early split freezes the wrong abstractions.

### When repository extraction is justified

Repository extraction is justified only when **all three** types of
readiness hold simultaneously, not just the technical one:

| Readiness type | What it covers |
|---|---|
| **Technical** | Slices 1-6 landed: schema crate, core crate, cgroup/platform boundary, fixture package boundary, archive verification without copying Assay internals |
| **Process** | CI lane classification stable, delegated proof recording stable, ringbuf diagnostics explicitly decided per [`#1271`](https://github.com/Rul1an/assay/issues/1271), 4-6 weeks without contract churn, maintainers can explain the boundary without archaeology |
| **Product** | Assay consumes Runner as an external dependency (Slice 6), at least one real non-spike external consumer or use case exists, the split is justified because someone needs Runner standalone — not because the split is technically possible |

The hardest gate is the **product** one. Slices 1-6 are work the
maintainers control; the external consumer is not. A roadmap that
treats the product gate as a foregone conclusion is wrong about its
own kill criteria above.

### What this section does NOT do

- It does not name an external consumer.
- It does not pre-approve a repository name, license, or publication
  target.
- It does not claim Runner is "released" or "published".
- It does not loosen any acceptance gate or v0 contract.
- It does not change the Slice sequence or the per-PR discipline
  rule above.

## References

- [Assay-Runner boundary and extraction map](boundary-map.md) — authoritative ownership and criteria
- [Runner platform and extraction readiness](platform-and-extraction-readiness.md) — current verdict and triggers
- [Runner artifact v0 contracts](artifacts-v0.md)
- [Runner acceptance fixture v0 contract](fixtures-v0.md)
- [Runner CI lane contract](ci-lanes.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Runner cross-runtime diff v0 contract](cross-runtime-diff-v0.md)
- [Runner cross-runtime diff v0 clean-output JSON Schema](schema/cross-runtime-diff-v0-clean.schema.json)
- [Assay-Runner Phase 1 acceptance](../../notes/ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md)
- Ring-buffer diagnostic projection follow-up: <https://github.com/Rul1an/assay/issues/1271>
