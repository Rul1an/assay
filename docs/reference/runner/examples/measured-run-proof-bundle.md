# Measured-Run Proof Bundle — Read-Only Walkthrough

> **Status:** explainable demo, not a runnable quickstart. Uses existing
> committed golden artifacts from the Phase 1 and Phase 2 acceptance
> fixtures. No install instructions, no eBPF setup, no claim that this is
> a standalone product.

This page walks through what one Assay-Runner measured run produces, using
the canonical golden artifacts already checked into the repo. The goal is
not to teach you how to run a measured run yourself — that requires a
delegated Linux/eBPF host class (`assay-bpf-runner`) — but to make it
concrete what comes *out* of one. If you are evaluating whether a
deterministic proof-bundle layer would be useful next to your existing
observability or testing setup, this is the document that answers "what
am I actually looking at?".

If you arrived here from
[GitHub Discussion #1329](https://github.com/Rul1an/assay/discussions/1329)
or the AgentSight
[Issue #44](https://github.com/eunomia-bpf/agentsight/issues/44),
this is the conceptual companion to the
[Phase 1 + 2 retrospective](../../notes/ASSAY-RUNNER-PHASE-1-AND-2-RETROSPECTIVE-2026-05-22.md).

## What This Is Not

- Not an install guide. There is no `cargo install assay-runner`.
- Not a live monitor. Nothing in this document streams.
- Not an instrumentation library. There is no SDK for the user to import.
- Not a comparison against any specific observability product. The point
  is to explain the *shape* of the artifact, not to position it against
  alternatives.

## What One Measured Run Produces

A measured run produces one archive (tar) containing five canonical
artifacts plus two ndjson layer streams. All five JSON artifacts are
content-addressed and verifiable through the existing Assay evidence
path. The schemas are frozen as `assay.runner.*.v0`.

```text
run-archive.tar
├── manifest.json                 # archive manifest with file digests
├── observation-health.json       # honest health-of-observation report
├── capability-surface.json       # what the run touched (paths/tools/decisions)
├── correlation-report.json       # SDK/policy/kernel correlation by tool_call_id
└── layers/
    ├── kernel.ndjson             # cgroup-scoped normalized kernel events
    ├── policy.ndjson             # MCP allow/deny decisions
    └── sdk.ndjson                # normalized SDK tool-call events
```

The five JSON files are what reviewers, CI gates, and cross-runtime diff
projections actually read. The ndjson streams are the layer-level evidence
the JSON files are computed from.

## Observation Health — Honesty About Gaps

This is the most important artifact. It says *what was observed cleanly
and what was not*. A measured run that lost kernel events, missed a
policy decision, or had a degraded SDK capture is required to say so
here.

Canonical golden, from
[`golden/observation-health-openai-agents-kernel-policy-v0.json`](../golden/observation-health-openai-agents-kernel-policy-v0.json):

```json
{
  "schema": "assay.runner.observation_health.v0",
  "run_id": "run_openai_agents_kernel_policy_determinism",
  "platform": "linux",
  "kernel_layer": "complete",
  "ringbuf_drops": 0,
  "policy_layer": "present",
  "sdk_layer": "self_reported",
  "cgroup_correlation": "clean",
  "notes": [
    "s2_kernel_capture: monitor_events=4 ringbuf_drops=0",
    "s4_policy_capture: policy_events=1",
    "s5_sdk_capture: sdk_events=3 sdk_tool_calls=1"
  ]
}
```

How to read this:

- `kernel_layer: complete` plus `ringbuf_drops: 0` means the kernel did
  not lose any events the eBPF ring buffer handed us. If it had, this
  would say `degraded` and the count would be non-zero, and the rest of
  the bundle would have to be interpreted in that light.
- `policy_layer: present` means MCP policy decisions were captured.
- `sdk_layer: self_reported` is the honest framing: SDK events come from
  the SDK itself, so we record them but never call them
  kernel-corroborated.
- `cgroup_correlation: clean` means the child process landed in the
  measured cgroup *before* it spawned, so the kernel observation window
  matches the process's actual lifetime.

If any of these degrade, the v0 contract requires the bundle to say so.
A measured run does not pretend to be cleaner than it is.

## Capability Surface — What The Run Touched

A normalized, set-shaped view of what the run did at the policy and
kernel layers. Deterministic across runs that do the same thing.

Canonical golden, from
[`golden/capability-surface-openai-agents-kernel-policy-v0.json`](../golden/capability-surface-openai-agents-kernel-policy-v0.json):

```json
{
  "schema": "assay.runner.capability_surface.v0",
  "run_id": "run_openai_agents_kernel_policy_determinism",
  "filesystem_paths": [
    "/tmp/assay-runner-openai-agents-kernel-policy/work/openai-agents-input.txt",
    "/tmp/assay-runner-openai-agents-kernel-policy/work/policy-input.txt"
  ],
  "network_endpoints": [],
  "process_execs": [],
  "mcp_tools": [
    "read_file"
  ],
  "policy_decisions": [
    "allow:read_file"
  ]
}
```

How to read this:

- The fixture used one MCP tool (`read_file`), policy allowed it once,
  and two filesystem paths were touched. No network, no extra process
  execs.
- Sets are sorted and deduplicated. Two runs of the same fixture produce
  byte-identical surfaces. Two runs that diverge in observed behaviour
  produce a non-empty diff on this artifact, which is exactly the
  regression signal CI gates can read.

This is the artifact a release gate would diff against a baseline. Not
"did the eval pass" — "did the surface change in a way we did not
expect".

## Correlation Report — Cross-Layer Binding

This is the artifact that makes the SDK / policy / kernel layers
*comparable* rather than three separate streams. It binds tool-calls
across layers by `tool_call_id`.

Canonical golden, from
[`golden/correlation-report-openai-agents-kernel-policy-v0.json`](../golden/correlation-report-openai-agents-kernel-policy-v0.json):

```json
{
  "schema": "assay.runner.correlation_report.v0",
  "run_id": "run_openai_agents_kernel_policy_determinism",
  "status": "clean",
  "bindings": [
    {
      "tool_call_id": "tc_runner_policy_001",
      "policy_decision": "allow",
      "kernel_event_count": 2,
      "window": {
        "start": "run_started",
        "end": "run_finished"
      }
    }
  ],
  "ambiguities": []
}
```

How to read this:

- The SDK declared a tool call with id `tc_runner_policy_001`. The
  policy layer recorded an `allow` decision under the same id. The
  kernel layer recorded two normalized events inside the binding
  window.
- `status: clean` means every SDK tool-call binding had a stable
  `tool_call_id` and a matching policy decision. If a runtime omitted
  `tool_call_id`, the v0 contract requires this to degrade to `partial`
  or `failed` with the ambiguity recorded — we do not invent ordering
  to paper over the gap.
- `window.start`/`window.end` are runner-defined phase markers from one
  canonical runner clock. SDK timestamps are informational only.

## SDK And Policy Layer Streams

The ndjson layers are not golden-checked-in (they are produced by the
fixture at acceptance time), but their shape is contract-frozen.
Illustrative slices from a clean run look like this.

SDK layer, one event per line, `assay.runner.sdk_event.v0`:

```ndjson
{"schema":"assay.runner.sdk_event.v0","run_id":"run_openai_agents_kernel_policy_determinism","seq":0,"event_type":"run_started","source":"openai-agents-fixture","sdk_name":"@openai/agents","sdk_version":"0.11.4"}
{"schema":"assay.runner.sdk_event.v0","run_id":"run_openai_agents_kernel_policy_determinism","seq":1,"event_type":"tool_call_started","source":"openai-agents-fixture","sdk_name":"@openai/agents","sdk_version":"0.11.4","tool_call_id":"tc_runner_policy_001","tool":"read_file"}
{"schema":"assay.runner.sdk_event.v0","run_id":"run_openai_agents_kernel_policy_determinism","seq":2,"event_type":"tool_call_completed","source":"openai-agents-fixture","sdk_name":"@openai/agents","sdk_version":"0.11.4","tool_call_id":"tc_runner_policy_001","tool":"read_file"}
```

Policy layer (also ndjson, MCP decision records). One illustrative entry:

```json
{
  "tool_call_id": "tc_runner_policy_001",
  "tool": "read_file",
  "decision": "allow",
  "reason": "policy:tools.allow",
  "ts_runner": "policy_layer_captured"
}
```

`tool_call_id` is the join key across all three layers. That is the
whole v0 correlation contract: one stable id, one window, three layers,
no inferred ordering.

## Cross-Runtime Diff — Comparing Two Runtimes

A cross-runtime diff projects the capability surface across two
different runtime fixtures (here `@openai/agents` vs `google-genai`),
applies the canonicalization rules from
[`cross-runtime-diff-decisions.md`](../cross-runtime-diff-decisions.md),
and produces a diff with explicit non-claims.

Excerpt from
[`golden/cross-runtime-diff-s5-gemini-v0.json`](../golden/cross-runtime-diff-s5-gemini-v0.json):

```json
{
  "schema": "assay.runner.cross_runtime_diff.v0",
  "base_runtime": "s5_openai_agents",
  "head_runtime": "gemini_google_genai",
  "status": "clean",
  "preconditions": {
    "base_health_clean": true,
    "head_health_clean": true,
    "stable_tool_call_ids_required": true,
    "stable_tool_call_ids_present": true,
    "runtimes_distinct": true
  },
  "surface": {
    "filesystem_paths": {
      "added":     ["<work>/gemini-input.txt"],
      "removed":   ["<work>/openai-agents-input.txt"],
      "unchanged": ["<work>/policy-input.txt"]
    },
    "mcp_tools": { "added": [], "removed": [], "unchanged": ["read_file"] },
    "policy_decisions": { "added": [], "removed": [], "unchanged": ["allow:read_file"] }
  },
  "sdk_metadata": {
    "comparison": "side_band_provenance",
    "base": { "sdk_name": "@openai/agents", "sdk_version": "0.11.4" },
    "head": { "sdk_name": "google-genai", "sdk_version": "2.6.0" }
  },
  "non_claims": [
    "cross_runtime_no_acceptability_judgment",
    "cross_runtime_no_declared_capability_input",
    "cross_runtime_no_derived_binding_identity",
    "cross_runtime_no_filename_semantic_equivalence",
    "cross_runtime_no_sdk_capability_equivalence"
  ]
}
```

How to read this:

- The two runtimes touched different per-fixture filename prefixes
  (`openai-agents-input.txt` vs `gemini-input.txt`) but reached the same
  policy decision on the same MCP tool (`read_file`, `allow`). That
  difference is in the surface diff, surfaced explicitly, not silently
  normalized away.
- The `<work>/` prefix is the A1 work-dir canonicalization rule:
  per-fixture work directories are normalized to a stable prefix so the
  diff is meaningful across runtimes without losing per-fixture
  filenames.
- `non_claims` is the heart of v0 honesty: the diff *does not* claim
  that two runtimes touching `read_file` means they have semantically
  equivalent capabilities, and it does not pretend to derive binding
  identity across runtimes. Cross-runtime equivalence is a separate,
  not-yet-opened, contract question.

If a schema change made these two runtimes diverge unexpectedly, the
diff catches it before it ships. That is the regression surface, and
it's the thing CI can read.

## What This Bundle Is Useful For

- **Release gating.** Diff today's surface against last release's
  baseline; non-empty diff blocks the release unless explicitly
  approved.
- **Regression testing.** Run the same agent fixture before and after
  a change; the surface should be byte-identical or the diff has to
  explain itself.
- **Cross-runtime comparison.** When the same prompt runs under two
  different agent runtimes, the cross-runtime diff says where they
  agree and where they don't, without making semantic-equivalence
  claims.
- **Honest evidence under load.** The observation health is the
  contract that the bundle is not lying about gaps. Degradation is
  recorded, not hidden.

What it is **not** useful for:

- "What is my agent doing right now" — Assay-Runner is not a live
  monitor. Use a system-level observability tool for that.
- Production traffic analysis — the contract is per-run, not per-fleet.
- Live LLM call observability — the supported path uses deterministic
  local providers; live LLM observability is a different problem.

## Where Each Artifact Comes From

If you want to read the code:

- Schemas and manifest types: `crates/assay-runner-schema/`
- Archive assembly and layer normalizers: `crates/assay-runner-core/`
- Cgroup placement primitives: `crates/assay-runner-linux/`
- Cross-runtime diff projection: `scripts/ci/` (currently script-hosted)
- Fixtures producing the goldens above: `runner-fixtures/openai-agents/`
  and `runner-fixtures/gemini-google-genai/`

All four runner crates are `publish = false`. The fixtures require a
delegated Linux/eBPF host class (`assay-bpf-runner`) to produce real
acceptance runs. The golden JSON artifacts referenced from this page
are checked-in snapshots so reviewers can read the contract without
running anything.

## Further Reading

- [Phase 1 + 2 retrospective](../../notes/ASSAY-RUNNER-PHASE-1-AND-2-RETROSPECTIVE-2026-05-22.md) — the long-form story
- [Assay-Runner reference index](../index.md) — all internal contracts
- [Runner artifact v0 contracts](../artifacts-v0.md) — full schema specs
- [Runner cross-runtime diff v0 contract](../cross-runtime-diff-v0.md)
- [Runner cross-runtime diff decisions (A1+B3+C1)](../cross-runtime-diff-decisions.md)
- [Phase 2D consolidation audit](../phase-2d-consolidation-audit.md) — extraction-readiness gating
