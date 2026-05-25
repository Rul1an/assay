# Cross-Runtime Drift — findings (synthetic-fixture baseline)

> **Status:** draft against **synthetic fixtures only**. The
> `compare/drift.py` MVP runs against
> [`compare/fixtures/{arm-a-openai,arm-b-gemini}/`](compare/fixtures)
> and produces the per-dimension drift report below. The Slice 3
> workflow (`.github/workflows/cross-runtime-drift-experiment.yml`)
> is ready to dispatch; once the maintainer dispatches it and
> commits the live baselines under
> [`runs/{a0,b0,drift}/`](runs/README.md), this document is
> updated in-place per the substitution procedure at the end —
> the synthetic numbers are *placeholders that prove the report
> shape works end-to-end*, not the published claim.
>
> **Last updated:** 2026-05-25.
>
> **Plan-doc:** [`../cross-runtime-drift-2026-05.md`](../cross-runtime-drift-2026-05.md)
> — research question, drift dimensions, threats to validity,
> sequencing.

## TL;DR

When the same agent task is executed by two different agent
runtimes under a single Runner capture boundary, the
capability_surface they produce is **not** structurally
identical — even after the workload contract is held constant.
The drift is bounded and falls into four classifications:

- **task-induced** — both runtimes touched the same surface
  element to satisfy the workload contract (`read_file`,
  `write_file`, `/usr/bin/node`, etc.).
- **provider-induced** — non-shared items match a provider-host
  whitelist (`api.openai.com`, `generativelanguage.googleapis.com`,
  ...). Attributable to the model provider's transport, not the
  framework choice.
- **runtime-induced** — non-shared items not on the provider
  whitelist and not in the fixture-path whitelist. Attributable
  to the runtime's loader, sidecar machinery, or vendored
  dependencies.
- **inconclusive** — either both arms have zero data on this
  dimension (the task didn't exercise it, e.g. `mcp_tools` under
  this contract), or one arm has zero data and the other has
  some (cannot tell whether the dimension is genuinely empty in
  the absent arm or just not measured). Matches `drift.py`'s
  two-pronged inconclusive rule verbatim.

The comparator (`compare/drift.py`) produces a per-dimension
drift report with this classification per row. The publishable
artefact is the **shape** of the report (which dimensions exist,
which classifications they support); the numbers themselves are
descriptive, not prescriptive.

## What the synthetic-fixture run shows

Running the comparator against
[`compare/fixtures/arm-a-openai`](compare/fixtures/arm-a-openai)
and
[`compare/fixtures/arm-b-gemini`](compare/fixtures/arm-b-gemini)
with the two `/tmp/work/fixture-{input,output}.txt` paths
declared as task-induced fixtures produces:

| Dimension | Source | Classification | Notes |
|---|---|---|---|
| `filesystem_paths_touched` | `capability_surface.filesystem_paths` | **runtime-induced** | Both arms touch the two fixture paths (in_both); each arm additionally touches its own `node_modules` tree and `dist/workload.js`. Six non-shared items, zero on the provider whitelist and zero on the fixture whitelist → runtime-induced. |
| `network_endpoints` | `capability_surface.network_endpoints` | **provider-induced** | `api.openai.com:443` on Arm A; `generativelanguage.googleapis.com:443` + `oauth2.googleapis.com:443` on Arm B. All three non-shared items match the provider-host whitelist (exact-or-subdomain match against `*.openai.com` / `*.googleapis.com`) → provider-induced. |
| `process_execs` | `capability_surface.process_execs` | **task-induced** | Both arms exec `/usr/bin/node`. Full overlap → task-induced. |
| `sdk_tool_events` | `layers/sdk.ndjson` (tool field, deduplicated) | **task-induced** | Both arms register `read_file` and `write_file`. Full overlap → task-induced. |
| `mcp_tool_surface` | `capability_surface.mcp_tools` | **inconclusive** | Empty in both arms. The workload contract forbids MCP servers, so this is *expected*-inconclusive, not surprising-inconclusive. |
| `tool_invocation_order` | `layers/sdk.ndjson` (seq-ordered) | **task-induced** | Both arms invoke `read_file → write_file` in that order, gated by the workload contract's required sequence. Identical sequence → task-induced. |

Summary on the synthetic data: **3 task-induced, 1
provider-induced, 1 runtime-induced, 1 inconclusive** — every
classification label is exercised at least once, which is the
acceptance criterion for the comparator MVP.

## Per-dimension narrative

### Filesystem drift is dominated by the runtime's `node_modules` shape

The synthetic Arm A fixture lists three `workload-openai`
paths under `node_modules/@openai/agents/...` and
`node_modules/zod/...`; Arm B lists three `workload-gemini` paths
under `node_modules/@google/genai/...` and
`node_modules/google-auth-library/...`. Same workload contract,
different dependency trees pulled in at runtime. This is the
sharpest evidence we have for "runtime choice has a measurable
filesystem cost beyond the task fixtures."

The classifier marks this `runtime-induced` because:

- The two fixture paths (`/tmp/work/fixture-input.txt`,
  `/tmp/work/fixture-output.txt`) appear in *both* arms — task
  overlap, but no longer the only thing in the row.
- The six non-shared paths don't match the provider-host
  whitelist (provider-host detection is gated to the
  `network_endpoints` row anyway).
- The six non-shared paths don't appear in the
  `--fixture-path` whitelist.
- Default: runtime-induced.

### Network drift cleanly separates provider transport from runtime machinery

Arm A's only outbound endpoint is `api.openai.com:443`. Arm B's
is `generativelanguage.googleapis.com:443` plus
`oauth2.googleapis.com:443` (Google's auth flow). All three are
provider-host matches; the row is `provider-induced` and the
detail makes the count explicit.

A subtler observation the synthetic fixture deliberately
exercises: the comparator's host-matching is *exact or
subdomain*, not substring. A path-shaped string containing
`api.openai.com` (e.g. a cache file named after the host) would
**not** match the provider whitelist when the same comparator
runs on the `filesystem_paths_touched` row, because the
provider-host classification is dimension-gated. This is the
correctness invariant the `NetworkEndpointParsingTests` cases in
[`compare/test_drift.py`](compare/test_drift.py) lock in.

### Process, SDK, and ordering rows are all task-induced on this contract

By construction. The workload contract requires `/usr/bin/node`
as the only exec, requires both arms to register `read_file` +
`write_file`, and requires the `read_file → write_file`
sequence. Drift here would indicate a contract violation, not
a runtime difference. The fact that these three rows are
**task-induced** is the success signal.

If a live run produces drift in any of these three rows, that's
a contract violation surfacing post-hoc — the per-iteration
contract-checker in the workflow is supposed to catch it
*before* the archive is uploaded.

### MCP layer is expected-inconclusive

The contract forbids MCP servers; `mcp_tools` is empty in both
arms. The comparator classifies that as `inconclusive` because
mechanically it cannot distinguish "intentionally empty" from
"unmeasured." A future variant of the experiment that explicitly
adds an MCP tool to both arms would turn this row into a real
signal; for the v0 contract, the row's `inconclusive` state is
honest.

## Live-data substitution procedure

When the maintainer dispatches
[`.github/workflows/cross-runtime-drift-experiment.yml`](../../../.github/workflows/cross-runtime-drift-experiment.yml)
and commits the resulting baselines:

1. Replace this status block with `Live baselines committed
   under runs/{a0,b0,drift}/ on <date>; n=3 per arm, all
   archives passed the health gate and the workload
   contract-checker.`
2. Replace the **What the synthetic-fixture run shows** table
   with the union of the three live `drift_pair_<i>.json`
   reports under `runs/drift/`. If a classification is stable
   across all three pairs, report it once; if it differs
   between pairs, note which iteration produced which label
   and why.
3. Replace the **Per-dimension narrative** with the live
   observations. The synthetic narrative is a *shape proof*;
   live narrative is the publishable claim.
4. Append a new section **What the live captures add over the
   synthetic baseline** that enumerates anything observed in
   the live data that the synthetic fixture did not predict
   (e.g. transient TCP connections to telemetry endpoints,
   loader-induced reads under `/proc`, etc.).
5. Leave Threats to Validity, "What still does NOT prove,"
   and Reproduction commands unchanged unless live data
   invalidates them.

## Threats to Validity

Copied verbatim from
[`../cross-runtime-drift-2026-05.md`](../cross-runtime-drift-2026-05.md);
will be revisited after the live captures.

1. **"Same contract" is a manual judgement.** Mitigated by the
   workload-contract checker, which runs per iteration in the
   workflow before any artifact upload. A contract violation
   fails the iteration; the run never ships as a baseline.
2. **Provider auth probes are not the runtime's fault.** The
   classifier marks them `provider-induced`, the detail names
   the count of provider matches. The narrative must keep this
   distinction in any published claim.
3. **Single-host bias.** All captures run on the same
   `assay-bpf-runner` VM. Kernel-specific quirks are constant
   across arms, but the result is not portable to other
   distros without re-running.
4. **One snapshot in time.** SDK versions move fast.
   [`workload-openai/package.json`](workload-openai/package.json)
   pins `@openai/agents` at `0.11.4`;
   [`workload-gemini/package.json`](workload-gemini/package.json)
   pins `@google/genai` at `2.6.0`. Re-run the experiment if
   either bumps a major.
5. **Capability_surface v0 granularity.** v0 records which
   paths were touched but not how many times or in what order.
   Drift on *what* is in scope; drift on *how often* is an
   explicit v2-comparator follow-up that requires parsing
   `layers/kernel.ndjson` directly.
6. **The drift report is not a security claim.** "Runtime B
   contacts an extra host" is a runtime-selection input, not a
   "runtime B is insecure" claim. Any published finding must
   repeat this up front.

## What still does NOT prove

Tracked alongside the runner-vs-otel-2026-05 "What still does
NOT prove" list — both experiments share the same v0
capability_surface scope and the same set of v2-comparator
follow-ups.

- **Live cross-runtime baselines.** This document is synthetic
  only until the Slice 3 workflow runs and the maintainer
  commits the baselines. The synthetic data proves the
  comparator and the classifier work end-to-end; it does *not*
  describe what the two runtimes actually do in production.
- **Read/write/create/remove classification.** Capability_surface
  v0 records "touched paths" undifferentiated. A v2 comparator
  that parses `layers/kernel.ndjson` could split these; deferred.
- **Per-path access counts and ordering.** Same v2-comparator
  follow-up.
- **Cross-distro portability.** All captures are
  Linux/kernel-specific. A future variant could re-run on a
  different kernel and report whether the drift labels are
  stable across hosts.
- **N > 3 stability.** The plan calls for n=3 for shape
  stability. If a live run shows drift labels that flip between
  iterations, bump to n≥5 and report the flip rate.
- **Latency or token-cost comparison.** Deliberately out of
  scope. Anyone reading this and wanting "is runtime X faster
  than runtime Y" is not asking the question this experiment
  answers.

## Reproduction commands

Stdlib-only on the comparator side. Workloads need Node 22+ and
their respective API keys; only the maintainer can dispatch the
live workflow.

```bash
export REPO_ROOT="$(git rev-parse --show-toplevel)"

# 1. Synthetic-fixture smoke run (no API keys, no Runner host).
python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/drift.py" \
  --archive-a "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-a-openai" \
  --archive-b "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-b-gemini" \
  --fixture-path /tmp/work/fixture-input.txt \
  --fixture-path /tmp/work/fixture-output.txt \
  --out-md /tmp/drift-smoke.md

# 2. Comparator + helpers unit tests (50 tests).
python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare" \
  -p 'test_*.py'

# 3. Contract-checker unit tests (13 tests).
python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/contract-checker" \
  -p 'test_*.py'

# 4. Live dispatch (maintainer only — requires OPENAI_API_KEY +
#    GOOGLE_API_KEY repo secrets and the assay-bpf-runner self-hosted
#    runner). After dispatch, download the three artefacts and commit
#    the baselines per runs/README.md.
```

## Pinned versions for this synthetic-fixture baseline

| Component | Pin |
|---|---|
| `@openai/agents` | `0.11.4` |
| `zod` | `4.1.13` |
| `@google/genai` | `2.6.0` |
| Node.js | `>= 22` |
| Python | stdlib only (`3.10+` for the type hints) |
| Runner archive schema | `assay.runner.archive_manifest.v0`, `assay.runner.capability_surface.v0`, `assay.runner.sdk_event.v0`, `assay.runner.observation_health.v0`, `assay.runner.correlation_report.v0` |
| Drift report schema | `assay.cross_runtime_drift.v0` |

## Non-claims (in case anyone reading this thinks the comparator says more than it does)

- The comparator does **not** rank runtimes. It describes drift.
- The comparator does **not** label any drift as a *bug* in
  either runtime. Runtime-induced filesystem drift is just
  evidence that the two runtimes carry different machinery; it
  is up to the consumer to decide whether that matters for their
  use case.
- The comparator does **not** claim its provider-host whitelist
  is exhaustive. New providers (or new endpoints for existing
  providers) need a `--provider-host` override.
- The comparator does **not** verify the binding between trace
  and archive — that is
  [`runner-vs-otel-2026-05/compare/compare.py`](../runner-vs-otel-2026-05/compare/compare.py)'s
  job, and intentionally so.
