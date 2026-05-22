# Assay-Runner Platform And Extraction Readiness

> Internal Phase 2B readiness checkpoint. This page records the current
> verdict on three deferred ambitions: repository extraction, macOS proof,
> and Windows proof. It is not a roadmap, not a schedule, and not a decision
> to open any of these lines.

The Phase 2B consolidation and the second-runtime entry plan deferred three
distinct ambitions:

- **Extraction**: moving the Assay-Runner candidate to its own repository
- **macOS proof**: producing measured-run acceptance on macOS hosts
- **Windows proof**: producing measured-run acceptance on Windows hosts

This page exists so "not now" cannot later be misread as "never discussed"
or as "apparently allowed now". Each line stays closed until a documented
readiness checkpoint passes.

## Status Overview

| Ambition | Current verdict | Documented in |
|---|---|---|
| Extraction (new repo) | **not ready** | [`boundary-map.md` § Extraction Readiness Criteria](boundary-map.md#extraction-readiness-criteria) |
| macOS proof | **not ready** | [`second-runtime-plan.md` § Out Of Phase 2B Scope](second-runtime-plan.md#out-of-phase-2b-scope), and several Non-Goals tables across Phase 2A docs |
| Windows proof | **not ready** | same as macOS, with the additional condition that macOS proof exists first |

Cross-runtime capability-diff is a separate Phase 2C question and is tracked
through the capability-diff documents, not here.

## Extraction Readiness

Authoritative checklist: [`boundary-map.md` § Extraction Readiness Criteria](boundary-map.md#extraction-readiness-criteria)
and [`boundary-map.md` § Extraction Blocking Conditions](boundary-map.md#extraction-blocking-conditions).
This page does not duplicate the 15 criteria. It only summarises the current
shape of the gap.

Current structural blockers, in priority order:

1. **Resolved by Phase 2D Slice 1.** `assay.runner.*.v0` schema data
   structures moved from `crates/assay-runner-spike/src/` to a new
   publish-disabled crate `crates/assay-runner-schema/`. The spike
   crate re-exports the moved types so existing call sites compile
   unchanged. See
   [`extraction-roadmap.md` § Slice 1](extraction-roadmap.md#slice-1--cratesassay-runner-schema)
   and the updated boundary-map ownership table.
2. **Resolved by Phase 2D Slices 1 + 2.** Archive manifest semantics
   (schema constants, `ArchiveFile`, `ArchiveManifest`) moved to
   `assay-runner-schema` in Slice 1. Archive assembly mechanics
   (`RunnerSpikeArchive`, `write`, normalizers, `RunSpec`
   orchestration) moved to `assay-runner-core` in Slice 2. The
   archive boundary conflict is now fully resolved on the runner
   side; verification continues through the existing Assay evidence
   path.
3. **Resolved by Phase 2D Slice 3.** Cgroup placement primitives
   (`CgroupManager`, `SessionCgroup`) moved from
   `crates/assay-cli/src/cgroup.rs` into a new publish-disabled crate
   `crates/assay-runner-linux/`. The runner candidate now consumes
   the cgroup API from a Linux platform adapter crate rather than
   from inside `assay-cli`. `crates/assay-cli/src/cli/commands/runner_spike.rs`
   imports `CgroupManager` and `SessionCgroup` directly from
   `assay_runner_linux`; `assay-runner-core` does not depend on
   `assay-cli` for cgroup placement. See
   [`extraction-roadmap.md` § Slice 3](extraction-roadmap.md#slice-3--cgroup-api-extraction).
4. There is no non-spike external consumer of the runner bundle format. The
   capability-diff projection helper is an internal consumer.

Triggers for reopening extraction readiness review:

- An external party concretely asks to consume the runner bundle format
  without depending on Assay internals. This creates the missing external
  consumer and creates a real reason to move schemas to a shared crate.
- The four structural blockers above are independently resolved.
- The `assay.runner.*.v0` contracts pass a stable window of at least four to
  six weeks without semantic churn.

Until those triggers fire, extraction stays closed by default. "We could
extract now" is not a trigger.

The forward-looking slice sequence that resolves the structural blockers
above is in
[`extraction-roadmap.md`](extraction-roadmap.md). That document does not
extract anything; it only sequences the work and binds every
Runner-impacting PR to a per-PR discipline rule that moves boundaries
toward extraction-readiness without weakening acceptance gates.

## macOS Proof Readiness

macOS is a separate platform spike, not a port of Linux. The Phase 1 spike
established the discipline for proving one platform end to end; the same
discipline applies to macOS independently.

Current blockers, in summary form:

- No proven Phase 2B second-runtime line yet. macOS work cannot ride on
  Phase 1 alone; we need at least one second-runtime fixture passing the
  capability-diff idempotent acceptance on Linux first.
- No `macos-measurement-spike-plan.md` analog of
  [`ASSAY-RUNNER-PHASE1-SPIKE-PLAN-2026-05-20.md`](../../notes/ASSAY-RUNNER-PHASE1-SPIKE-PLAN-2026-05-20.md).
  macOS needs its own kill criteria and acceptance criteria because the
  measurement technology is different.
- No documented measurement-technology decision. macOS measurement
  candidates differ from eBPF in capability, signing requirements, and
  evidence boundary. The decision belongs in the macOS spike plan, not in
  the existing Linux artifact contracts.
- No dedicated host class equivalent to the `assay-bpf-runner` for macOS.

Triggers for opening macOS proof readiness review:

- A second-runtime fixture has reached `qualifies` and produced a clean
  idempotent capability-diff on Linux.
- The Linux runner-boundary has passed a stable window of at least four to
  six weeks.
- A concrete use case requires macOS measurement (not "we could", but
  "we need this for X").
- Resources are available to provision a dedicated macOS host class.

Until those triggers fire, macOS work is not a port question and not a
small extension. It is a separate spike with its own entry plan.

## Windows Proof Readiness

Windows is downstream of macOS in two senses:

- The measurement-technology question is broader (ETW, eBPF-for-Windows,
  Sysmon, and others each carry different trade-offs).
- Cross-platform `assay.runner.*.v0` schemas should first survive Linux plus
  macOS before they are stress-tested against a third platform.

Triggers for opening Windows proof readiness review:

- macOS proof has reached clean acceptance with its own spike plan and host
  class.
- A concrete use case requires Windows measurement.

No technology decision is made by this page.

## Non-Goals

This page does not:

- open extraction, macOS, or Windows work
- propose a schedule, quarter, or calendar window
- choose a macOS measurement technology
- choose a Windows measurement technology
- propose a new repository name or layout
- open issues for any of the three ambitions
- weaken the Linux/eBPF acceptance bar
- replace the authoritative criteria in `boundary-map.md`

Each of those decisions belongs in its own dedicated planning slice, only
after the relevant triggers above fire.

## How To Use This Page

When a maintainer or reviewer is tempted to start extraction, macOS, or
Windows work:

1. Read the relevant Triggers section above.
2. If no trigger applies, do not open the line. Reference this page.
3. If a trigger applies, open a dedicated planning slice analogous to
   `second-runtime-plan.md`, with kill criteria, acceptance criteria, and
   non-goals specific to that ambition. Do not start implementation work
   from this page.

This page is updated only when a verdict changes, when a structural blocker
is resolved, or when a trigger fires.

## References

- [Assay-Runner boundary and extraction map](boundary-map.md)
- [Runner second runtime Phase 2B plan](second-runtime-plan.md)
- [Runner capability-diff v0 contract](capability-diff-v0.md)
- [Runner CI lane contract](ci-lanes.md)
- [Assay-Runner Phase 1 spike plan (template for future platform spikes)](../../notes/ASSAY-RUNNER-PHASE1-SPIKE-PLAN-2026-05-20.md)
- [Assay-Runner Phase 1 acceptance](../../notes/ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md)
