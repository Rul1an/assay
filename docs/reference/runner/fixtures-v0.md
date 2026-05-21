# Runner Acceptance Fixture v0 Contract

> Internal Phase 2A reference. This page freezes the fixture discipline behind
> the delegated Linux/eBPF Phase 1 proof. It is not a public Assay-Runner
> fixture API.

Runner acceptance fixtures are small deterministic programs used to prove the
normalized runner artifacts. They are not examples of general agent behavior.
Their job is to create a stable measured-run surface that exercises kernel,
policy, and SDK correlation without introducing accidental host noise.

## Current Fixture Set

| Fixture path | Layer exercised | Role |
|---|---|---|
| `tests/fixtures/runner-spike/kernel-only-agent.sh` | kernel | deterministic filesystem and process evidence |
| `tests/fixtures/runner-spike/mcp-policy-agent.sh` | policy plus kernel | deterministic MCP `read_file` policy decision |
| `tests/fixtures/runner-spike/openai-agents-sdk-policy-agent.sh` | SDK plus policy plus kernel | combines the real OpenAI Agents SDK fixture with the policy fixture |
| `tests/fixtures/runner-spike/openai-agents-js/fixture-agent.js` | SDK | deterministic local-model OpenAI Agents tool call |

The three delegated determinism wrappers run the relevant acceptance path
three times and compare normalized artifacts byte-for-byte.

## Fixture Contract Versus Accepted Instances

This page separates two layers:

- **Fixture contract:** rules that every runner acceptance fixture must follow:
  deterministic invocation, stable identifiers, no live secrets, normalized
  artifact determinism, and explicit telemetry-versus-evidence boundaries.
- **Accepted fixture instance:** the exact shape proven by the current Phase 1
  full S5 fixture, including SDK version, event counts, tool name, tool-call id,
  and health-note text.

Changing the general fixture discipline is a contract change. Updating a
single accepted instance, for example during an `@openai/agents` dependency
bump, is still reviewable but should be handled as an instance update as long
as the general contract remains intact.

## Accepted Full S5 Fixture Instance

The Phase 1 `openai-agents-kernel-policy` acceptance path fixes the full
kernel plus policy plus SDK fixture shape deliberately:

| Surface | Required v0 shape | Failure mode |
|---|---|---|
| Health | `kernel_layer=complete`, `ringbuf_drops=0`, `cgroup_correlation=clean`, `policy_layer=present`, `sdk_layer=self_reported` | delegated acceptance fails before determinism comparison |
| Health note | `s5_sdk_capture: sdk_events=3 sdk_tool_calls=1` | delegated acceptance fails with `sdk capture note missing` |
| Policy stream | exactly one normalized policy event | delegated acceptance fails with `expected one policy event, got N` |
| SDK stream | exactly three normalized SDK events | delegated acceptance fails with `expected three sdk events, got N` |
| SDK sequence numbers | contiguous `seq` values starting at `0` | SDK parsing fails with a sequence-mismatch error before archive acceptance |
| SDK event order | `tool_call_started`, `tool_call_completed`, `run_finished` | delegated acceptance fails with `sdk event sequence mismatch` |
| SDK source | `openai-agents-js-fixture` unless explicitly overridden for diagnosis | delegated acceptance fails with `sdk event N source mismatch` |
| SDK package | `sdk_name=@openai/agents`, `sdk_version=0.11.4` for the accepted v0 fixture | delegated acceptance fails with `sdk_name mismatch` or `sdk_version mismatch` |
| Tool | `read_file` | delegated acceptance fails with `sdk tool mismatch` or policy tool mismatch |
| Tool-call binding | one shared id: `tc_runner_policy_001` | delegated acceptance fails with SDK, policy, or binding `tool_call_id` mismatch |
| Correlation report | `status=clean`, `ambiguities=[]`, one binding | delegated acceptance fails with status, ambiguity, or binding-count mismatch |
| Correlation window | `{"start":"run_started","end":"run_finished"}` | delegated acceptance fails with `binding window mismatch` |

These counts and values are part of the accepted v0 S5 fixture instance.
Changing them is not a copy-edit; it changes the deterministic instance that
proved Phase 1 and requires an explicit fixture-instance review.

## Contract Principles

1. **Fixtures are evidence generators.** Each fixture should create a small,
   intentional set of filesystem, process, policy, and SDK events that the
   normalizer can claim as attribution evidence.
2. **Determinism is below output level.** It is not enough for the final files
   to match. Cold-cache and warm-cache runs must produce the same normalized
   evidence artifacts.
3. **Control paths stay out of evidence.** Temporary policy files, request
   JSONL, response JSONL, dependency trees, dynamic-loader paths, and locale
   probes are fixture plumbing, not capability evidence.
4. **No live secrets or live LLM calls.** The OpenAI Agents fixture uses a
   deterministic local model provider and must not require API credentials.
5. **Stable identifiers are part of the fixture.** `run_id`, tool names,
   policy decision summaries, SDK event schemas, and `tool_call_id` values must
   be explicit and stable.
6. **The wrapper owns reset semantics.** Three-run wrappers must reset the
   measured work directory between runs when create-vs-open-existing behavior
   could change normalized kernel evidence.
7. **The normalizer owns evidence boundaries.** Fixtures should avoid
   needless noise, but telemetry-versus-evidence filtering remains part of the
   runner normalizer contract.

## Invocation Contract

Acceptance fixtures that execute as programs use:

```text
<fixture> <work-dir>
```

Rules:

- require exactly one work-directory argument
- fail non-interactively with a non-zero exit code on misuse
- write deterministic fixture files below the provided work directory
- avoid wall-clock timestamps, random suffixes, hostnames, absolute temp paths,
  or dependency-version strings in evidence-bearing outputs
- keep temporary control files outside the measured work directory when they
  are not part of the attribution claim
- avoid background work that can outlive the fixture process

The delegated runner CLI owns cgroup placement for the fixture process tree.
Fixtures must not move themselves between cgroups or spawn detached processes
outside the measured process tree.

## Environment Contract

The wrappers may provide layer-specific environment variables. A fixture must
validate required variables before doing measured work.

| Variable | Used by | Semantics |
|---|---|---|
| `ASSAY_RUNNER_RUN_ID` | policy and SDK fixtures | shared run id for emitted events |
| `ASSAY_BIN` | policy fixture | CLI binary used to wrap the MCP file server |
| `ASSAY_RUNNER_POLICY_DECISION_LOG` | policy fixture | policy event log path |
| `ASSAY_RUNNER_SDK_EVENT_LOG` | SDK fixture | SDK event log path |
| `ASSAY_RUNNER_SDK_EVENT_SCHEMA` | SDK fixture | expected SDK event schema string |
| `ASSAY_RUNNER_SDK_TOOL_CALL_ID` | SDK plus policy fixtures | stable tool-call id used for v0 correlation |

The OpenAI Agents fixture also sets `OPENAI_AGENTS_DISABLE_TRACING=1`. The
runner bundle does not claim OpenAI tracing export behavior in Phase 2A.

Environment values that can affect fixture output or normalized evidence are
part of fixture review. New fixture wrappers should pin or explicitly justify
each stability point:

| Review point | Current v0 source | Risk if it drifts |
|---|---|---|
| `TZ=UTC` | review requirement; not part of the evidence claim unless a wrapper sets it | wall-clock formatting can drift at timezone or DST boundaries |
| `LANG=C` and `LC_ALL=C` | review requirement; locale paths are filtered from evidence | localized tool output can change bytes and path probes |
| stable `TMPDIR` | wrappers use `${TMPDIR:-/tmp}` for run and control roots | host migration can move control paths or leak absolute temp paths into diagnostics |
| stable `HOME` | delegated runner user default; fixtures must not read user config as evidence | SDK/package tooling can observe user-local config or paths |
| fixed Node major/minor line | Node 22+ preflight plus fixture dependency review | runtime startup behavior and SDK hooks can change across Node lines |
| stable current working directory | acceptance scripts `cd` to the repository root before dispatching fixtures | relative paths and package metadata lookup can drift after script refactors |
| stable `umask` | host default unless fixture file modes become evidence | file-mode-sensitive evidence or future golden artifacts can drift |

The accepted v0 fixture does not claim general locale, timezone, or user-home
coverage. It claims deterministic normalized artifacts for the delegated Linux
host and the fixture environment asserted by the acceptance wrappers.

## Filesystem Evidence Contract

Evidence-bearing filesystem paths should be deterministic and scoped to the
provided work directory. The current v0 fixtures use fixed names such as:

- `input.txt`
- `output.txt`
- `policy-input.txt`
- `openai-agents-input.txt`

Fixtures may create control files in `/dev/shm` or `${TMPDIR:-/tmp}` when the
files are not attribution evidence. Those control paths must not be asserted in
`capability-surface.json`.

When a fixture writes an input file only if it is missing, the three-run wrapper
must still reset the measured work directory before each run if kernel evidence
would otherwise differ between file creation and file reuse.

## Policy Fixture Contract

Policy fixtures should:

- call exactly the intended MCP tool for the proof mode
- use deterministic JSON-RPC ids
- pass a stable `_meta.tool_call_id` for v0 SDK-to-policy correlation
- write policy decisions to `ASSAY_RUNNER_POLICY_DECISION_LOG`
- assert the wrapped response includes the deterministic fixture content

Policy-denied paths remain evidence when the policy decision is the claim,
even if ordinary kernel `openat` telemetry for the same path would be filtered
as loader/runtime noise.

## SDK Fixture Contract

SDK fixtures should emit normalized SDK events with:

- stable schema string
- shared `run_id`
- contiguous `seq` values starting at zero
- stable `source`
- installed SDK package name and version loaded from package metadata
- stable tool names
- stable `tool_call_id` on tool-call events

The Rust SDK event parser permits non-tool lifecycle events such as
`run_finished` without tool-call fields, but `tool_call_started` and
`tool_call_completed` must include both `tool_call_id` and `tool`. The full S5
acceptance then requires the SDK tool-call id to equal the policy
`tool_call_id` and the correlation binding id.

For Phase 2A v0, `tool_call_id` is required for clean SDK-to-policy correlation
of tool-call events. Call-id-less agent support is deliberately undecided and
tracked in <https://github.com/Rul1an/assay/issues/1275>. Do not merge a
correlation contract slice that silently chooses an order-based fallback
without resolving that issue.

The OpenAI Agents fixture must keep tool concurrency bounded to one. A future
fixture that exercises parallel tool calls is a new contract test, not a small
edit to the v0 deterministic fixture.

## Three-Run Determinism Contract

Three-run wrappers must compare normalized artifacts, not raw telemetry:

- `observation-health.json`
- `capability-surface.json`
- `correlation-report.json`
- relevant normalized layer streams under `layers/`

The full S5 determinism wrapper compares exactly:

- `observation-health.json`
- `capability-surface.json`
- `correlation-report.json`
- `layers/sdk.ndjson`
- `layers/policy.ndjson`

Wrappers should print self-describing diffs when these artifacts drift. The
diff is diagnostic only; it must not loosen the pass condition.

The v0 machine-readable golden shapes for these artifacts are listed in
[`golden/index.md`](golden/index.md). They are canonical examples for field
presence and serialization shape; their example values are illustrative unless
the artifact contract explicitly defines the value vocabulary. The delegated
three-run comparison remains the executable determinism check for real fixture
instances.

Passing delegated determinism requires:

- `kernel_layer=complete` when kernel capture is in scope
- `ringbuf_drops=0`
- `cgroup_correlation=clean`
- stable normalized evidence across all three runs
- no delegated skip treated as success

## Correlation Clock Rule

Kernel, policy, and SDK correlation windows use runner-defined phase markers
derived from the measured run lifecycle, not SDK-provided wall-clock timestamps.
SDK timestamps are informational only and are never the primary join key for v0
correlation. Choosing a concrete runtime clock source such as
`CLOCK_MONOTONIC` is runner-side mechanics and belongs in the boundary map
before it becomes a v0 artifact contract requirement.

## Dependency Upgrade Contract

Fixture dependencies are part of the evidence surface when they affect emitted
SDK events, hook names, package metadata, or policy correlation. For
`@openai/agents` bumps:

1. update `tests/fixtures/runner-spike/openai-agents-js/package.json` and
   `tests/fixtures/runner-spike/openai-agents-js/package-lock.json` together
2. verify the fixture can load installed package metadata
3. update the expected SDK version assertion
   (`ASSAY_RUNNER_ACCEPTANCE_EXPECT_SDK_VERSION`, defaulted by the acceptance
   wrapper) in the same change
4. run ordinary CI
5. dispatch `Runner Spike Delegated` with
   `gates=openai-agents-kernel-policy` and `build_ebpf=true`
6. record the delegated run URL and commit SHA in the PR

Review the deterministic `fixture-agent.js` model path for API or hook-name
breakage whenever the SDK is bumped. In particular, confirm the fixture still
emits the required three-event sequence and still maps OpenAI Agents tool-call
ids into `assay.runner.sdk_event.v0`.

Dependency bumps must not relax event-schema validation, sequence validation,
or three-run determinism.

When the bump arrives as a Dependabot PR, follow the maintainer steps in the
[Dependabot lane flow](dependabot-lane-flow.md) for delegated-proof recording.

## Second-Order Fixtures

Negative, adversarial, or S7-style fixtures are second-order contract tests.
They are useful before widening the runner claim, but they must not silently
complicate the happy-path S5 acceptance fixture. The happy path remains the
small deterministic proof that kernel, policy, and SDK evidence can be
captured, correlated, and reproduced byte-for-byte.

## Adding Or Changing A Fixture

Before merging a fixture change, reviewers should be able to answer:

- What new evidence value does this fixture intentionally add?
- Which normalized artifact should change?
- Which paths are control plumbing and should not become evidence?
- Is `tool_call_id` stable, or is this blocked on issue #1275?
- Does the fixture run without network credentials or live LLM calls?
- Does the narrow delegated gate pass?
- If the change touches shared capture, cgroup, monitor, normalizer, or
  archive behavior, did `gates=all` pass?

If the answers are unclear, treat the change as runner-impacting and run the
highest applicable delegated gate from the CI lane contract.

## Non-Goals

The v0 fixture contract does not define:

- macOS or Windows attribution fixtures
- live LLM/cassette behavior
- parallel tool-call correlation
- call-id-less fallback semantics
- production-load or long-running process behavior
- external plugin or third-party SDK fixture authoring

Each of those requires a separate contract decision before it can become part
of Assay-Runner.
