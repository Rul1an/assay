# Assay-Runner Boundary And Extraction Map

> Internal Phase 2A reference. This page defines the current Assay-Runner
> boundary candidate after the delegated Linux/eBPF Phase 1 proof. It is not a
> repository-split plan and does not create a released product surface.

Phase 1 proved that Assay can produce deterministic measured-run bundles on a
delegated Linux/eBPF host. Phase 2A keeps that proof reviewable while the
runner boundary is consolidated. Extraction is only possible after the
boundary is stable; it is not the goal of this document.

## Core Rule

Assay remains the owner of artifact semantics.

Runner may own measured execution, layer capture orchestration, cgroup-scoped
process placement, and correlation mechanics. Runner must not become an
independent authority for what an Assay evidence artifact means.

Implications:

- artifact schemas stay owned by Assay core/reference docs
- archive verification stays an Assay responsibility
- runner-generated bundles must remain verifiable through Assay evidence
  semantics
- runner projection or capability diff may consume artifacts, but must not
  redefine them

## Boundary Table

| Layer | Stays In Assay Core | Runner Candidate Owns | Shared Contract |
|---|---|---|---|
| Archive semantics | manifest shape, digest semantics, bundle verification | archive assembly for measured runs | archive manifest schema and run-id consistency |
| Observation health | field set, status values, strict health semantics | computing health from capture results | `observation-health.v0` |
| Capability surface | artifact schema, evidence categories, deterministic set serialization | deriving the surface from normalized runner events | `capability-surface.v0` |
| Correlation report | schema, status values, ambiguity semantics | producing bindings from SDK/policy/kernel windows | `correlation-report.v0` |
| Kernel monitor and eBPF | monitor implementation, BPF programs, stats model | selecting capture window, cgroup scope, and normalizer filters for runner proof | monitor event schema, drop accounting, cgroup health |
| Policy decisions | MCP/proxy policy semantics and decision event shape | including policy logs in measured-run archives | policy event schema and `tool_call_id` extraction |
| SDK events | normalized SDK event schema | shim-specific event adapters and fixture capture | `assay.runner.sdk_event.v0` |
| Acceptance fixtures | fixture contract and delegated acceptance semantics | fixture programs and control paths for runner proof | fixture v0 contract |
| CI discipline | repository-required checks and ordinary CI | delegated proof lane selection and run recording | CI lane contract |
| Operational runbook | security posture and hosted runner policy | delegated host procedure and failure triage | delegated runbook |
| Capability diff | Trust Basis / Harness projection semantics | measured-run capability input bundles | future Phase 2B diff contract |

## Dependency Direction

Allowed dependency direction:

```text
Assay core semantics
  -> runner measured execution
  -> harness/report projection
```

Forbidden direction:

```text
runner implementation detail
  -> redefines Assay artifact meaning
```

Runner code may depend on Assay core crates or exported contracts. Assay core
must not depend on a publishable runner crate for artifact interpretation. If a
core crate needs a helper currently living in runner-spike code, move the
helper to the appropriate core module first and cover that move with tests.

Additional extraction rule:

1. If a runner candidate module contains artifact schema or data-structure
   definitions needed by both Assay and a future runner repository, those
   definitions must migrate to an Assay-owned shared contract before
   extraction. The runner may fill those structures, but it may not be the sole
   owner of their meaning across a repository boundary.

## Current In-Repo Ownership

The current spike surfaces remain in `Rul1an/assay`:

| Path | Current role | Boundary classification |
|---|---|---|
| `crates/assay-runner-schema/` | publish-disabled v0 schema data structures (Phase 2D Slice 1) | shared contract owned by the Runner extraction line; the data half of the runner v0 contract layer |
| `crates/assay-runner-schema/src/health.rs` | observation-health data structures and validation | shared contract; hosted by the schema crate since Slice 1 |
| `crates/assay-runner-schema/src/surface.rs` | capability-surface data structures and deterministic set storage | shared contract; hosted by the schema crate since Slice 1 |
| `crates/assay-runner-schema/src/correlation.rs` | correlation-report data structures and validation | shared contract; hosted by the schema crate since Slice 1 |
| `crates/assay-runner-schema/src/sdk_event.rs` | SDK event schema string and `SdkLayerEvent` shape | shared contract; hosted by the schema crate since Slice 1 |
| `crates/assay-runner-schema/src/archive_manifest.rs` | archive manifest schema string, archive file path constants, `ArchiveFile`, `ArchiveManifest` | shared contract (manifest semantics half of the archive boundary conflict); hosted by the schema crate since Slice 1 |
| `crates/assay-runner-core/` | publish-disabled runner mechanics crate (Phase 2D Slice 2) | runner candidate orchestration, archive assembly, and layer normalizers; consumes `assay-runner-schema` for v0 types; the mechanics half of the runner v0 contract layer |
| `crates/assay-runner-core/src/run.rs` | measured command execution, run id, archive handoff | runner candidate orchestration; hosted by the core crate since Slice 2 |
| `crates/assay-runner-core/src/kernel.rs` | kernel capture normalization and health application | runner candidate mechanics using Assay monitor semantics; hosted by the core crate since Slice 2 |
| `crates/assay-runner-core/src/policy.rs` | policy log normalization and binding into the archive | runner candidate mechanics using Assay policy semantics; hosted by the core crate since Slice 2 |
| `crates/assay-runner-core/src/sdk.rs` | SDK ndjson parsing and SDK/policy mismatch marking | runner candidate mechanics; consumes `SdkLayerEvent`/`SDK_EVENT_SCHEMA` from the schema crate; hosted by the core crate since Slice 2 |
| `crates/assay-runner-core/src/archive.rs` | runner archive assembly and writing | runner candidate mechanics; consumes manifest types from the schema crate; hosted by the core crate since Slice 2 |
| `crates/assay-runner-spike/` | removed legacy alias crate (post-Slice 6B cleanup) | removed after proving no in-workspace consumers. Historical extraction docs may still reference the alias wrapper as pre-removal context |
| `crates/assay-runner-linux/` | publish-disabled Linux platform adapter crate (Phase 2D Slice 3) | **runner Linux placement primitives only**; currently hosts cgroup v2 placement (`CgroupManager`, `SessionCgroup`). Phase 2D Slice 4 confirmed this boundary: the eBPF monitor adapter and the kernel programs stay in their existing Assay-owned crates (see below); macOS/Windows adapters are out of scope until separate platform spikes open under `platform-and-extraction-readiness.md` |
| `crates/assay-runner-linux/src/cgroup.rs` | cgroup v2 placement primitives | hosted by the Linux platform crate since Slice 3 (previously at `crates/assay-cli/src/cgroup.rs`) |
| `crates/assay-cli/src/cgroup.rs` | removed in Phase 2D Slice 3 | the placement primitives moved to `assay-runner-linux`; `crates/assay-cli/src/cli/commands/runner_spike.rs` now imports `CgroupManager`/`SessionCgroup` from `assay_runner_linux` |
| `crates/assay-monitor/` | monitor reader, stats, event decoding | **Shared Assay measurement substrate.** Used by the runner candidate (`assay-runner-core` consumes `MonitorStatsSnapshot`) AND by Assay's own `assay monitor` standalone CLI command. Slice 4 explicitly confirms this stays Assay-owned and is not relocated into `assay-runner-linux`, because moving it would break the non-runner Assay consumers |
| `crates/assay-ebpf/` | eBPF programs | **Assay-owned Linux kernel program substrate.** Loaded by `assay-monitor`. Slice 4 explicitly confirms this stays Assay-owned; it is the kernel-side counterpart of the shared measurement substrate, not a runner-private adapter |
| `crates/assay-cli/src/cli/commands/runner_spike.rs` | **Temporary runner composition layer** (until Phase 2D Slice 6). Hidden CLI command that wires together `assay-runner-schema` (data), `assay-runner-core` (mechanics), `assay-runner-linux` (cgroup), and `assay-monitor` (events). | This composition lives in `assay-cli` because no public Runner entrypoint exists yet. Slice 6 (Assay-consumes-Runner-as-external) decides whether a public runner entrypoint replaces this composition or whether `assay-cli` continues to host it across the extraction boundary. Until then, this file is treated as a runner-candidate-adjacent surface in the lane-check classifier (Gate.ALL via the explicit-paths rule) |
| `crates/assay-evidence/**` | evidence artifact verification and existing bundle semantics | Assay core artifact semantics |
| `crates/assay-core/**` | MCP, policy, runtime, and shared decision semantics | Assay core semantics |
| `runner-fixtures/gemini-google-genai/` | Gemini Python google-genai second-runtime fixture package (Phase 2D Slice 5A) | runner-owned fixture asset structured as a separate package; moved from `tests/fixtures/runner-spike/gemini-google-genai/` so the fixture boundary is visible at the top of the tree, mirroring an eventual extracted runner repo layout |
| `runner-fixtures/openai-agents/` | S5 OpenAI Agents (`@openai/agents`) accepted-fixture package (Phase 2D Slice 5B) | runner-owned fixture asset; moved + renamed from `tests/fixtures/runner-spike/openai-agents-js/` (the `-js` language suffix dropped because the fixture identity is the runtime). The SDK source-identity emitted by `fixture-agent.js` was renamed from `openai-agents-js-fixture` to `openai-agents-fixture` in the same slice to keep the package boundary, directory name, and recorded evidence consistent |
| `tests/fixtures/runner-spike/` | deterministic shared cross-runtime helpers (kernel-only fixture, mcp-policy-agent, mcp_file_server, sdk-event-wrapper, sdk-policy-agent); both runtime fixtures (S5 OpenAI Agents and Gemini) moved out to `runner-fixtures/` in Slices 5A and 5B | candidate runner fixtures under shared fixture contract; a later Slice 5C may relocate the shared helpers themselves if a clean package boundary for them emerges |
| `scripts/ci/runner-spike-*.sh` | acceptance and determinism wrappers | candidate runner gates under shared CI lane contract |
| `.github/workflows/runner-spike-delegated.yml` | manual delegated Linux/eBPF workflow | repository-owned proof lane |
| `docs/reference/runner/*.md` | Phase 2A contracts | shared internal contracts |
| `docs/ops/ASSAY-RUNNER-DELEGATED-RUNBOOK-2026-05-21.md` | delegated host runbook | shared operational contract |

## Active Boundary Conflicts

These are the known hard cases. Do not resolve them by moving code first and
explaining the ownership later.

| Conflict | Why it is hard | Current rule |
|---|---|---|
| `archive.rs` | the runner assembles archives, but manifest shape, digest meaning, and verification are artifact semantics | Fully resolved by Phase 2D Slices 1 + 2: manifest schema constants, `ArchiveFile`, and `ArchiveManifest` moved to `assay-runner-schema` in Slice 1; assembly (`RunnerSpikeArchive`, `RunnerSpikeArchiveError`, `write`) moved to `assay-runner-core` in Slice 2. Archive verification continues to use the existing Assay evidence path; the runner side no longer mixes assembly mechanics with manifest semantics ownership |
| `health.rs` and `observation-health.json` | the runner measures drops and cgroup health, but `kernel_layer=complete` and related status meanings are Assay claims | Assay owns the status definitions; runner computes whether a measured run satisfies them |
| telemetry-versus-evidence filters | the runner implements event-type and path filters, but deciding what counts as evidence is artifact semantics | Assay owns the evidence taxonomy and rationale; runner owns the implementation and diagnostics |
| `tool_call_id` fallback semantics | fallback would change correlation mechanics and `correlation-report.json` ambiguity semantics at the same time | v0 clean correlation and the first Phase 2B capability-diff contract require stable tool-call ids; call-id-less support is out of scope until a separate fallback contract exists |

If a future PR touches one of these conflicts, reviewers should require a
contract update or an explicit statement that the PR does not change the
boundary.

## What Must Never Move Alone

The following cannot be moved to a separate runner repository without an
explicit shared-contract replacement:

- artifact schema definitions
- archive verification semantics
- observation-health status meanings
- telemetry-versus-evidence filter rationale
- CI lane decision table
- fixture v0 contract
- delegated proof acceptance note

If extraction happens later, these must either stay in Assay core or become a
versioned shared contract package consumed by both repositories.

## Extraction Readiness Criteria

Do not create a separate Assay-Runner repository until all of the following are
true:

| Check | Ready when |
|---|---|
| Contract stability | artifact, fixture, CI-lane, and boundary contracts have no semantic churn for one consolidation window |
| CI classification | runner-impacting changes can be classified by the CI lane contract without discussion |
| Delegated proof recording | delegated workflow run URL, commit SHA, selected gate, and result are recorded on runner-impacting PRs |
| Shared schema ownership | `assay.runner.*.v0` schemas have an owner and release/versioning path importable by both Assay and any future runner repo |
| Dependency direction | no Assay core crate depends on a publishable runner crate for artifact interpretation |
| CLI coupling | runner orchestration does not require private `assay-cli` types or hidden command internals across a repo boundary |
| Cgroup API | cgroup placement uses a stable API rather than `assay-cli`-local helpers |
| Monitor API | runner capture can depend on a stable monitor API without copying monitor/eBPF internals |
| Archive verification | runner archives verify through Assay evidence semantics without copying verification logic |
| Boundary API stability | two consecutive minor releases, or an equivalent internal stabilization window, pass without breaking the boundary API |
| External consumer | at least one non-spike consumer can read the runner bundle contract without depending on spike implementation details |
| Call-id decision | call-id-less correlation is explicitly excluded from the extracted v0 scope unless a later fallback contract replaces that rule |
| Drop diagnostics | the ring-buffer debug follow-up in
   <https://github.com/Rul1an/assay/issues/1271> is either implemented or
   explicitly accepted as post-extraction operational work |
| CI enforcement path | the Assay-Runner lane-check required status and
   reviewer workflow are active; future refinements are tracked separately
   rather than blocking the v0 boundary |
| Maintainer explainability | a maintainer can explain which crate owns each boundary row above without reading the Phase 1 history |

If the boundary map remains materially unstable after a 4-6 week consolidation
window, treat that as evidence that extraction is premature. Do not force a
repository split while the ownership line is still moving.

The consolidation window is an evidence requirement, not a calendar
requirement. After Phase 2D Slices 1-6B landed, the passive 4-6 week wait
is replaced by the burn-in criteria defined in
[`phase-2d-consolidation-audit.md`](phase-2d-consolidation-audit.md). The
burn-in criteria are the new satisfaction condition for this consolidation
gate; they are not necessarily satisfied yet — the audit page tracks which
criteria are observed and which are still pending. Counting weeks without
observing repo behavior is weak evidence; the audit page makes the
evidence concrete.

## Extraction Blocking Conditions

Extraction is blocked if any of these are true:

- `assay.runner.*.v0` schemas still need breaking changes to remain useful
- delegated CI selection still depends on reviewer intuition rather than the
  CI lane contract
- fixture stability requires undocumented local knowledge
- `tool_call_id` fallback semantics are being decided implicitly
- runner schema definitions are still coupled to Assay internals in ways a
  future runner repository would need to import
- runner orchestration still depends on `assay-cli`-local cgroup helpers
  without a stable API boundary
- runner crate build boundaries require more Assay internals than a
  deliberate shared schema/monitor contract allows
- macOS or Windows support is being treated as a port rather than a separate
  platform spike
- a separate repo would need to copy Assay evidence verification logic
- runner code cannot be tested without private host state that is absent from
  the delegated runbook
- there is no non-spike consumer of the runner bundle format

## Future Split Shape

If extraction becomes justified, the likely shape is:

| Area | Future owner |
|---|---|
| Artifact schemas and verification | Assay core or shared contract crate |
| Monitor/eBPF substrate | Assay core unless runner needs a separately versioned monitor package |
| Runner orchestration CLI | Assay-Runner candidate |
| Deterministic runner fixtures | Assay-Runner candidate, governed by shared fixture contract |
| Delegated CI workflow | Depends on repository ownership of the dedicated runner |
| Capability diff projection | Assay-Harness or a later Runner/Harness shared surface |

The first extracted release, if it ever exists, should be narrow: Linux/eBPF
delegated measured runs with the existing `none`, `kernel+policy`, and
OpenAI Agents fixture paths. It should not bundle macOS, live LLM calls,
fleet operations, or OTel mappings into the first boundary.

## Non-Goals

This boundary map does not:

- choose an external product name
- create a new repository
- promise publication or packaging
- define macOS or Windows measurement
- define live LLM or cassette semantics
- define OTel mappings
- change delegated acceptance criteria

Those decisions require separate contracts after the Linux boundary is stable.
