# Assay-Runner Product Candidate Memo

> **Status:** internal product-candidate memo, Phase-1-spike gated, not a
> public roadmap commitment
> **Date:** 2026-05-18
> **Last updated:** 2026-05-21
> **Scope:** records a possible measured-run product track beside
> Assay-Harness; no external publication, repo split, or build commitment is
> implied.

This note defines a candidate product track for measured agent execution. It
is intentionally separate from Assay-Harness, which remains the recipe, gate,
and projection layer above Assay artifacts.

## Product Layout

```text
Assay-core    = evidence, policy, monitor, Trust Basis primitives
Assay-Harness = recipes, gates, PR projections
Assay-Runner  = measured execution, tri-layer bundles, capability diff
```

Three products, three roles, no overlap. None of these products define new
artifact semantics outside Assay-core.

**Name discipline:** `Assay-Runner` is an internal placeholder only. External
name work is deferred until a Phase 1 spike passes. Public names such as
`Assay Measure` or `Assay Run Attestation` should not be advanced before there
is one verified demo bundle and one credible capability diff.

## Thesis

The harness should stop asking the agent what it did. It should observe side
effects at the syscall boundary, bind evidence under a content-addressed,
verifiable bundle, and diff the resulting capability surface. The agent runtime
is interchangeable.

The kernel layer is not treated as perfect truth. It is the primary observation
source for side effects, with explicit health metadata that says how complete
or incomplete that observation was.

## Differentiator

| Category, May 2026 | Trust basis | What is missing |
|---|---|---|
| Eval harnesses such as Inspect AI, METR, promptfoo, DeepEval | SDK or score | Observation below the SDK |
| Agent-SDK harnesses such as OpenAI Agents SDK, LangGraph | SDK is source of truth | Kernel-blind, runtime-bound |
| Sandbox-as-harness such as Daytona, E2B, Modal | Container boundary | Inside-the-run observation |
| Gateway harnesses such as Kuadrant MCP, Envoy ext_proc | Proxy visibility | Filesystem, syscall, process view |
| Observability such as Langfuse, Helicone, LangSmith | SDK-reported traces | Verification and capability diff |
| **Assay-Runner** | **Correlated syscall, policy, and SDK evidence** | Phase 1 must prove attribution is reliable |

The opening is kernel-grounded, verify-before-diff capability evidence across
multiple runtimes.

## Integration Claim

The needed substrate already exists in Assay. Runner is correlation and
integration, not a new research bet.

| Capability | Source |
|---|---|
| Syscall observation, LSM, tracepoints | `crates/assay-monitor`, `crates/assay-ebpf` |
| Tier 1 policy, kernel-oriented | `crates/assay-policy` |
| Tier 2 policy, userspace and proxy | `crates/assay-policy`, `crates/assay-mcp-server` |
| Content-addressed bundles | `crates/assay-evidence` |
| Verify-before-diff, Trust Basis | `crates/assay-evidence` |
| Deterministic LLM replay | `assay-core::vcr` |
| Landlock sandbox | `assay sandbox` |
| Attack and chaos simulation | `crates/assay-sim` |

What is missing is the correlation layer (`run_id` to cgroup to pid to
`tool_call_id`) and the shim layer.

## Bounded Claim

The Runner bundle is observation evidence, not transcript truth.

It may state:

- what the kernel observed in a marked time window bound to `run_id` via
  cgroup v2
- which policy decisions `assay-mcp-server` or `assay-policy` made in that
  same window
- which tool-call events the SDK shim emitted in that same window, when a
  shim other than `none` is used

It must not claim:

- that this is everything the agent did
- that the SDK layer is correct
- that the kernel layer captured every relevant event
- that layer timestamps prove causal ordering

For v1, cross-layer claims are set-based. They are not sequence claims.

## Observation Health

Observation health is a first-class bundle field. Every bundle carries explicit
metadata like:

```yaml
observation_health:
  kernel_layer: complete | partial_ringbuf_drops | absent
  ringbuf_drops: <int>
  policy_layer: present | absent
  sdk_layer: present | self_reported | absent
  cgroup_correlation: clean | partial | failed
```

A bundle with `kernel_layer=absent`, such as a macOS run, can still be valid.
It is valid but explicitly incomplete. A bundle with `ringbuf_drops>0` says
kernel visibility was partial, and every capability diff projected from that
bundle must show that warning. The diff must never hide how complete its own
evidence is.

## Linux-First Boundary

Full kernel-grounded measurement is Linux only:

- tri-layer bundle
- capability diff over the kernel layer
- eventual production measurement path

Partial or degraded mode is allowed on macOS and Windows:

- SDK plus policy layers where available
- `kernel_layer=absent`
- every output marks the incomplete trust basis explicitly

This is a design boundary, not a vague cross-platform promise.

## Risks

| Risk | What it breaks |
|---|---|
| Cross-layer correlation ambiguity | If tool-call to policy to syscall attribution is not reliable, the value proposition fails. |
| Baseline instability | Noisy capability sets make PR diffs meaningless without set summarization and ignore rules. |
| Overhead under load | Bundle writing must stream to disk, not aggregate large event sets in memory. |
| Shim drift | Each major SDK version needs an owned shim contract, not best-effort broad compatibility. |
| Compliance-frame contamination | Event-level signed receipts would drag the product into the wrong category. Bundle-level signing can stay optional. |

The real risk is attribution, not eBPF or bundle writing.

## Phase 1: Proof Spike

Two shims are required, in order of proof:

- `--agent-shim none`: the epistemic wedge. Runner asks nothing of the SDK.
- `--agent-shim openai-agents`: the adoption wedge. The SDK layer becomes a
  correlated source, not the source of truth.

The `none` shim has two explicit submodes:

- `none + kernel-only`: no SDK shim and no Assay policy layer
- `none + kernel+policy`: no SDK shim, but Assay policy/proxy events are
  present and correlated with kernel-observed side effects

Spike question:

> Can Assay produce one verifiable run bundle per shim mode, where every
> observable layer is correlated to the others with low ambiguity, and
> observation health is honestly self-reported?

### Acceptance Criteria For `none`

1. One run has one explicit `run_id`.
2. Kernel-layer events inside cgroup C and window T are grouped into a
   capability surface: filesystem prefixes, network endpoints, and process
   execs.
3. In `none + kernel+policy`, policy-layer events correlate with kernel-layer
   events: policy saw tool X, and the kernel saw a congruent syscall set in
   the same bounded window.
4. In `none + kernel-only`, `policy_layer: absent` is explicit.
5. `observation_health` is correctly filled. Three consecutive runs of the
   same scenario produce consistent health metadata.
6. `assay evidence verify` succeeds.

### Acceptance Criteria For `openai-agents`

These are in addition to the `none` criteria:

7. SDK-layer `tool_call_id` correlates with a policy-layer decision carrying
   the same `tool_call_id`.
8. The SDK-layer tool-call window contains the kernel syscall set for that
   tool, set-based only.
9. Three consecutive runs produce the same tool to policy to syscall binding.

### Kill Criterion

If the spike is too ambiguous or too noisy, the track stops.

Examples:

- tool to syscall binding succeeds in only 60 percent of runs
- PID recycling or cgroup disturbance makes attribution unstable
- ring-buffer loss makes ordinary runs visibly incomplete

In that case, this memo becomes the documented dead branch. No further product
build follows.

### Phase 1 Result

Phase 1 passed on delegated Linux/eBPF hardware on 2026-05-21. The acceptance
record is
[`ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md`](./ASSAY-RUNNER-PHASE1-ACCEPTANCE-2026-05-21.md).

This result moves the internal track from proof spike to Phase 2A
consolidation. It still does not imply a public product launch, repository
split, hosted service, macOS support, or live LLM support.

## Phase 2: Capability Diff

Only start Phase 2 if Phase 1 passes.

The v1 discipline is set-based summaries only. No ordered traces, no sequence
semantics.

First output shape:

```text
New kernel-observed capabilities vs baseline:

+ network    api.stripe.com:443
+ filesystem /var/cache/agent-state/
+ process    /usr/bin/git
+ mcp_tool   filesystem.write_file

Observation health:
  kernel_layer       complete
  ringbuf_drops      0
  policy_layer       present
  sdk_layer          self_reported  (openai-agents v0.11.1)
  cgroup_correlation clean
```

Output rules:

- The health block is mandatory. There is no capability diff without
  observation health.
- A diff from `kernel_layer=partial_ringbuf_drops` must carry a visible
  warning.
- Summaries are set-based: touched X, Y, and Z. They never claim "first X,
  then Y."
- Projection text is descriptive, not normative. It says what changed, not
  whether the change is acceptable.
- Baseline pinning and project-level ignore rules are required before broad
  use.

This is where Runner and Harness meet: Runner produces the bundle; Harness
diffs and projects it.

## Phase 3: Extra Shims

Only after Phase 2 should cross-runtime language become externally claimable.
At that point, add at least two shims beyond `openai-agents`.

Candidates:

- Inspect AI
- LangGraph
- Mastra

Each shim maps into one normalized SDK-layer event schema.

## Phase 4: Continuous Run Measurement

Do not put this in the public story until Phase 1 and Phase 2 are credible.

The long-term shape is the same bundle schema in production, with the same diff
mechanism for "today versus yesterday". That is a future bridge, not v0.1
sales material.

## Decisions Deferred

1. External name. No naming work before the spike produces one demo.
2. Repo host. A spike can live inside `Rul1an/assay`; do not create
   `Rul1an/Assay-Runner` before Phase 1 evidence exists.
3. Whether `none` eventually remains pure kernel-only by default or defaults
   to kernel plus policy when Assay MCP wrapping is available.

## Verdict

Ship this internally as a candidate memo, not a roadmap. The product track is
interesting only if the Phase 1 attribution spike passes. If attribution is
ambiguous, the track should stop cleanly.
