# Assay-Runner Phase 1 Spike Plan

> **Status:** internal execution plan, spike only, not a public roadmap
> commitment
> **Date:** 2026-05-20
> **Scope:** expands the Phase 1 proof spike from
> [`ASSAY-RUNNER-PRODUCT-CANDIDATE-2026-05-18.md`](./ASSAY-RUNNER-PRODUCT-CANDIDATE-2026-05-18.md);
> no external name, repo split, hosted service, or product build is implied.

This plan turns the Assay-Runner candidate memo into a bounded spike contract.
It answers one question:

> Can Assay produce one verifiable measured-run bundle per shim mode, with
> low-ambiguity layer correlation and honest observation health?

If the answer is no, this track stops.

## Non-Goals

Phase 1 must not become a hidden product launch. It explicitly does not include:

- public naming work beyond the internal `Assay-Runner` placeholder
- a new public repository
- a hosted service, dashboard, or production sidecar
- Phase 2 capability diff as a PR comment
- sequence semantics or causal ordering across layers
- event-level signatures or signed tool-call receipts
- cross-platform full measurement claims

The only deliverable is a local spike proving or disproving attribution.

## Placement

Spike code should live in `Rul1an/assay`, not in a new repository.

Allowed source layout:

| Path | Purpose | Public promise |
|---|---|---|
| `crates/assay-runner-spike/` | Optional publish-disabled orchestration crate for the spike | None |
| `crates/assay-cli/src/cli/commands/runner_spike.rs` | Hidden CLI entrypoint, if needed for demos | None |
| `tests/fixtures/runner-spike/` | Tiny deterministic programs and MCP fixtures | None |
| `docs/notes/ASSAY-RUNNER-PHASE1-SPIKE-PLAN-2026-05-20.md` | This plan | Internal only |

If a crate is added, its name must include `spike` and it must be
publish-disabled. A hidden test harness is also acceptable if that proves the
attribution question with less surface area. Do not create a publishable
`assay-runner` crate before Phase 1 passes.

## Existing Surfaces To Reuse

The spike should be mostly assembly:

| Need | Existing surface |
|---|---|
| Linux event capture | `crates/assay-monitor::Monitor` |
| cgroup scope | `Monitor::set_monitored_cgroups`, `Monitor::attach_network_cgroup` |
| ring-buffer health | `MonitorStatsSnapshot::total_ringbuf_dropped` |
| process tree fallback | `crates/assay-monitor::tree::ProcessTreeTracker` |
| policy decision identity | `crates/assay-core/src/mcp/proxy/decisions.rs::extract_tool_call_id` |
| policy decision event | `DecisionEvent` in `crates/assay-core/src/mcp/decision_next/event_types.rs` |
| bundle run-id discipline | `crates/assay-evidence` bundle writer and verifier |
| deterministic bundle check | `assay evidence verify` |
| LLM replay | `assay-core::vcr` |
| sandbox baseline | `assay sandbox` and Landlock docs |
| adversity inputs | `crates/assay-sim` attack and chaos patterns |

Do not duplicate these semantics inside the spike unless there is no callable
boundary yet.

Known integration gaps:

- `extract_tool_call_id` is not currently a stable public API for a separate
  spike crate. S4 may require a small `assay-core` adapter or visibility
  relaxation before correlation code can call it directly.
- Assay currently has more than one internal `extract_tool_call_id` function.
  Before S4 correlation work begins, choose one canonical extraction path for
  policy events and spike-side correlation, then cover that choice with a
  contract test.

## Spike Output Contract

The spike writes one local archive:

```text
assay-runner-spike-<run_id>.tar.gz
```

The archive should be accepted by `assay evidence verify` or by a clearly named
temporary verifier if the existing bundle writer cannot carry every layer yet.
If a temporary verifier is needed, the plan must record the gap before any
Phase 2 work begins.

Minimum file shape:

```text
manifest.json
events.ndjson
layers/kernel.ndjson
layers/policy.ndjson
layers/sdk.ndjson
capability-surface.json
observation-health.json
correlation-report.json
```

Allowed empty files:

- `layers/policy.ndjson` when `policy_layer=absent`
- `layers/sdk.ndjson` when `sdk_layer=absent`

No layer may be silently omitted.

## Observation Health Contract

Phase 1 freezes this v0 shape:

```json
{
  "schema": "assay.runner.observation_health.v0",
  "run_id": "run_...",
  "platform": "linux",
  "kernel_layer": "complete",
  "ringbuf_drops": 0,
  "policy_layer": "present",
  "sdk_layer": "absent",
  "cgroup_correlation": "clean",
  "notes": []
}
```

Allowed values:

| Field | Values |
|---|---|
| `kernel_layer` | `complete`, `partial_ringbuf_drops`, `absent` |
| `policy_layer` | `present`, `absent` |
| `sdk_layer` | `present`, `self_reported`, `absent` |
| `cgroup_correlation` | `clean`, `partial`, `failed` |

Rules:

- `ringbuf_drops > 0` forces `kernel_layer=partial_ringbuf_drops`.
- non-Linux runs force `kernel_layer=absent`.
- `--agent-shim none` forces `sdk_layer=absent`.
- an SDK shim may only set `sdk_layer=self_reported` unless kernel or policy
  evidence corroborates the specific event boundary.
- `cgroup_correlation=failed` is a failing Phase 1 run, even if the bundle
  verifies.

## Capability Surface V0

Phase 1 summarizes kernel observations into deterministic sets:

```json
{
  "schema": "assay.runner.capability_surface.v0",
  "run_id": "run_...",
  "filesystem_prefixes": ["/tmp/assay-runner-spike/"],
  "network_endpoints": ["127.0.0.1:8080"],
  "process_execs": ["/usr/bin/curl"],
  "mcp_tools": ["filesystem.read_file"],
  "policy_decisions": ["allow:filesystem.read_file"]
}
```

Serialization must be deterministic:

- sort every set lexicographically before writing JSON
- use stable path normalization rules
- use stable endpoint formatting: `host_or_ip:port`
- no timestamps in set entries
- no event ordering claims

This prevents byte-level diff noise before Phase 2 exists.

## Correlation Model

Phase 1 correlation is set-based and window-based.

Required keys:

| Key | Source | Purpose |
|---|---|---|
| `run_id` | runner-spike | binds all layers into one stream |
| `cgroup_id` or `cgroup_path` | runner-spike / monitor | kernel scope anchor |
| `pid` / process tree | monitor | child process grouping and fallback debugging |
| `tool_call_id` | MCP proxy / SDK shim | policy and SDK join key |
| `window_start` / `window_end` | runner-spike | bounded attribution window |

Timing rule:

- all runner-owned windows use one canonical clock source; on Linux, prefer a
  monotonic clock for joins
- SDK-shim timestamps are recorded as self-reported informational fields only
  and must not be used as the sole basis for kernel or policy attribution

Allowed claim:

```text
Tool-call T occurred in window W, policy saw T, and kernel events K were
observed inside the measured cgroup during W.
```

Forbidden claim:

```text
Tool-call T caused syscall S at timestamp N.
```

## Workstreams

### S0: Freeze The Spike Contract

Deliverables:

- this plan merged
- one follow-up issue or checklist for each workstream
- no code changes yet

Exit gate:

- second-party internal review agrees the spike is bounded and can be killed
  without product fallout; if no second party is available, record a written
  self-checklist in this note family before code work starts

### S1: Runner Boundary And `run_id`

Implement the minimal orchestration boundary:

```text
assay runner-spike run --agent-shim none -- <command...>
```

Hidden CLI is acceptable. A direct test harness is also acceptable if adding a
CLI surface creates too much churn.

Responsibilities:

- generate one `run_id`
- create or identify one cgroup scope on Linux
- launch the child command inside that scope
- record `window_start` and `window_end`
- write an initial manifest and empty layer files

Acceptance:

- one command run produces one archive
- archive includes `run_id` in every file that has a run identity
- non-Linux produces a degraded bundle with `kernel_layer=absent`

### S2: Kernel Layer And Health

Use `assay-monitor` rather than adding new probes.

Responsibilities:

- configure monitored cgroup
- listen to monitor events during the run window
- snapshot monitor stats before and after the run
- compute `ringbuf_drops`
- write `layers/kernel.ndjson`
- derive `filesystem_prefixes`, `network_endpoints`, and `process_execs`

Acceptance:

- a fixture that opens a known file records that file prefix
- a fixture that executes a known binary records that binary
- a fixture that connects to a local TCP endpoint records that endpoint where
  monitor support exists
- three repeated runs produce the same health metadata and same capability set
  for the deterministic fixture

### S3: `none + kernel-only`

This is the epistemic proof path.

Fixture:

```text
tests/fixtures/runner-spike/kernel-only-agent.sh
```

The fixture should:

- read one file under a temp directory
- write one file under a temp directory if write events are observable
- execute one stable binary such as `/usr/bin/env`
- optionally connect to a local listener

Acceptance:

- `sdk_layer=absent`
- `policy_layer=absent`
- `kernel_layer=complete` on Linux when no drops occur
- `cgroup_correlation=clean`
- the bundle verifies with `assay evidence verify` or, until runner-spike
  archives are carried by `assay-evidence`, with the temporary
  `scripts/ci/runner-spike-kernel-only-acceptance.sh` verifier
- three repeated fixture runs verify with
  `scripts/ci/runner-spike-kernel-only-three-run-determinism.sh`

### S4: `none + kernel+policy`

This proves Runner still works when the agent uses Assay policy surfaces but
does not use an SDK shim.

Fixture:

```text
tests/fixtures/runner-spike/mcp-policy-agent.sh
```

The fixture should route one MCP tool call through `assay mcp wrap` or a
minimal existing MCP proxy path.

Acceptance:

- `sdk_layer=absent`
- `policy_layer=present`
- policy event includes a stable `tool_call_id`
- kernel capability set is congruent with the allowed tool call
- correlation report joins policy to kernel by `run_id`, window, and cgroup
- the fixture verifies with
  `scripts/ci/runner-spike-kernel-policy-acceptance.sh`
- three repeated fixture runs verify with
  `scripts/ci/runner-spike-kernel-policy-three-run-determinism.sh`

### S5: `openai-agents` Shim

This is adoption proof, not epistemic proof.

Transport is fixed for the spike:

- runner-spike launches `node` as a subprocess
- a small JS wrapper owns the `@openai/agents` SDK invocation
- the wrapper writes normalized SDK events as NDJSON to an explicit file or
  file descriptor supplied by runner-spike
- runner-spike consumes that stream and writes `layers/sdk.ndjson`
- stdout/stderr remain diagnostic channels; they are not the canonical SDK
  event stream

The first S5 implementation slice may ingest a prewritten normalized SDK
event log through the same hidden runner boundary. That only freezes the
`assay.runner.sdk_event.v0` contract; it is not enough to claim the
`openai-agents` shim has passed until the subprocess transport and
deterministic fixture are wired.

The next S5 implementation slice proves the transport boundary with a
deterministic SDK fixture:

```text
tests/fixtures/runner-spike/sdk-event-wrapper.sh
scripts/ci/runner-spike-sdk-contract-acceptance.sh
```

Runner supplies `ASSAY_RUNNER_SDK_EVENT_LOG`, `ASSAY_RUNNER_RUN_ID`, and
`ASSAY_RUNNER_SDK_EVENT_SCHEMA` to the measured subprocess. The subprocess
writes normalized SDK NDJSON to that path; stdout and stderr remain
diagnostic only.

The following S5 slice cross-checks SDK self-report against the existing
policy layer:

```text
tests/fixtures/runner-spike/sdk-policy-agent.sh
scripts/ci/runner-spike-sdk-policy-correlation.sh
scripts/ci/runner-spike-sdk-policy-three-run-determinism.sh
scripts/ci/runner-spike-sdk-policy-mismatch.sh
scripts/ci/runner-spike-sdk-policy-mismatch-three-run-determinism.sh
```

When a policy layer is present, every SDK tool-call id must match an existing
policy correlation binding. A missing binding marks the correlation report
partial with `sdk_tool_call_without_policy_binding:<tool_call_id>`. SDK events
still do not create bindings or promote kernel/policy claims by themselves.
The mismatch verifier intentionally emits a distinct SDK tool-call id and must
observe that partial-correlation ambiguity across three deterministic runs.

The first real JavaScript SDK slice runs the same correlation gate through a
Node subprocess that imports `@openai/agents` directly and uses a local
deterministic model provider to force one `read_file` function-tool call:

```text
tests/fixtures/runner-spike/openai-agents-js/fixture-agent.js
tests/fixtures/runner-spike/openai-agents-sdk-policy-agent.sh
scripts/ci/runner-spike-openai-agents-sdk-policy-correlation.sh
```

This proves the `@openai/agents` runtime hook path and the runner-supplied SDK
event stream without a live LLM request. It still is not the full delegated
Linux kernel+policy+SDK acceptance gate: that requires the privileged eBPF host
to run the kernel capture alongside the JavaScript shim.

Keep it thin:

- emit normalized SDK events
- carry `tool_call_id`
- record SDK package/version when available
- never promote SDK events to verified side effects by themselves
- keep the wrapper killable with the measured process tree

Normalized SDK events:

```text
tool_call_started
tool_call_completed
run_finished
run_failed
```

Determinism is mandatory:

- no live LLM calls are allowed in the spike suite
- S5 uses `assay-core::vcr` replay in strict mode, for example
  `ASSAY_VCR_MODE=replay_strict`, with a checked-in cassette or fixture
- if replay cannot be wired through `@openai/agents`, use a deterministic
  degenerate SDK fixture and record that blocker before claiming S5 pass

Acceptance:

- `sdk_layer=self_reported`
- `tool_call_id` matches policy-layer `tool_call_id` where MCP policy is used
- SDK events arrive through the subprocess NDJSON stream, not through ad hoc
  log scraping
- the S5 fixture runs in replay-strict or deterministic fixture mode
- kernel event set for the tool window is stable across three runs
- SDK-only data cannot make `kernel_layer=complete`

### S6: Correlation Report

Write a machine-readable report:

```json
{
  "schema": "assay.runner.correlation_report.v0",
  "run_id": "run_...",
  "status": "clean",
  "bindings": [
    {
      "tool_call_id": "tc_001",
      "policy_decision": "allow",
      "kernel_event_count": 3,
      "window": {
        "start": "2026-05-20T00:00:00Z",
        "end": "2026-05-20T00:00:01Z"
      }
    }
  ],
  "ambiguities": []
}
```

Status values:

- `clean`: all expected joins present
- `partial`: a layer is absent or a join is incomplete but declared
- `failed`: attribution is too ambiguous for the run to count

Acceptance:

- no successful Phase 1 demo may have `status=failed`
- ambiguity reasons are deterministic strings, not free-form diagnostics only

### S7: Adversarial Spike Checks

Use `assay-sim` patterns or small local fixtures to stress the attribution
model before declaring Phase 1 successful.

Required checks:

- child process forks before doing the file access
- child exits quickly after access
- two sibling processes touch different files in the same run
- policy-denied tool call produces a policy event without claiming matching
  allowed side effects
- induced or simulated ring-buffer pressure sets
  `kernel_layer=partial_ringbuf_drops`

Exit gate:

- if any normal deterministic fixture produces unstable attribution across
  three runs, Phase 1 fails

## Scenario Matrix

| Scenario | Shim | Policy | Expected health | Required proof |
|---|---|---|---|---|
| raw file/process run | `none` | absent | kernel complete on Linux | stable kernel capability set |
| raw run on non-Linux | `none` | absent | kernel absent | valid degraded bundle |
| MCP allowed tool | `none` | present | kernel complete, policy present | policy to kernel congruence |
| MCP denied tool | `none` | present | policy present | no false allowed side-effect claim |
| OpenAI Agents tool call | `openai-agents` | present where possible | SDK self-reported plus policy/kernel | stable tool to policy to kernel binding |
| ringbuf pressure | any | any | kernel partial | visible health warning |

## Hard Kill Criteria

Stop the track if any of these are true after two implementation passes:

- cgroup correlation cannot be made `clean` for the raw deterministic fixture
- policy-to-kernel attribution is unstable across three repeated runs
- `tool_call_id` cannot be carried through the OpenAI Agents path without
  brittle SDK internals
- ordinary runs produce ring-buffer drops often enough that `complete` is not
  the normal Linux result
- bundle verification requires a parallel artifact system instead of an
  incremental extension to `assay-evidence`

If the track stops, write a short closure note instead of continuing into Phase
2.

Closure note path:

```text
docs/notes/ASSAY-RUNNER-PHASE1-CLOSURE-<date>.md
```

The closure note is the sole deliverable when the track stops. It must record
the failed criterion, the evidence observed, and whether any spike code should
be deleted or left quarantined for later reference.

## Success Criteria

Phase 1 passes only when all of the following are true:

- one `none + kernel-only` bundle verifies
- one `none + kernel+policy` bundle verifies
- one `openai-agents` bundle verifies or has a documented SDK blocker that
  does not affect the `none` proof path
- all three bundle types include observation health
- deterministic fixtures are stable across three repeated Linux runs
- adversarial checks do not break attribution silently
- all successful outputs are explicit about absent, partial, and self-reported
  layers

Passing Phase 1 does not authorize a public product launch. It only authorizes
Phase 2 planning for capability diff.

## Phase 2 Handoff, If Phase 1 Passes

The next plan should be `ASSAY-RUNNER-PHASE2-CAPABILITY-DIFF-PLAN-*`.

It must start from Phase 1 artifacts and answer:

- how capability surfaces are compared
- how ignore rules are expressed
- how Harness projects the diff
- which warnings block or annotate PR output
- which outputs are canonical JSON versus reviewer Markdown

Do not start Phase 2 until Phase 1 has real bundles checked into a fixture or
artifact location.

## Review Checklist

- The plan keeps Runner separate from Assay-Harness.
- The spike can be killed cleanly.
- `none` remains the first proof path.
- observation health is mandatory.
- deterministic serialization is required before diff work.
- no public naming, repo split, or external claims are introduced.
