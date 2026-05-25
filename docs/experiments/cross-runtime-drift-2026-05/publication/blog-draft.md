# Same agent task, two runtimes, one Runner capture: what does the drift look like?

> **Status:** draft. Not yet published. Lives in the repo so the
> framing matches the evidence on disk and so reviewers can verify
> every number from the committed artefacts.
>
> **Caveat up front:** the tables below are from live Linux/eBPF
> captures on `assay-bpf-runner`, n=3 per arm. The synthetic fixtures
> still exist, but only as comparator smoke tests.
>
> **Date drafted:** 2026-05-25.

Most agent-observability writing in 2026 is about *one* runtime at
a time. Pick OpenAI Agents, pick LangChain, pick LangGraph, pick
CrewAI, instrument it with OTel/OpenInference spans, ship the
spans to whatever backend, reason from there. That works.

The question that kept coming up internally was different: if
two different agent runtimes execute the same agent task, and
you capture both runs under the same kernel-level boundary —
what is the *structural* difference in what they touched?

Not "which one is faster" — that question is uninteresting and
benchmark-shaped. Not "which one is safer" — that question is
loaded and wrong-shaped. Just: when you point two runtimes at
the same workload contract, where does the surface drift, and
why?

This is the question the `cross-runtime-drift-2026-05`
experiment in [`Rul1an/assay`](https://github.com/Rul1an/assay)
tries to answer. Everything in this post is verifiable from
[`docs/experiments/cross-runtime-drift-2026-05/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/cross-runtime-drift-2026-05)
— including a stdlib-only Python comparator
(`compare/drift.py`, 53 unit tests passing) that takes two
Runner archives and produces the table shown below.

## TL;DR for runtime-selection folks

The comparator's per-dimension drift report can represent drift in
**four shapes** by design. The live n=3 run exercised three of them
stably: task-induced, runtime-induced, and inconclusive. The synthetic
fixture still exercises provider-induced as a smoke-test shape, but the
live network data exposed a v0 boundary: the archive carries IP
endpoints, not provider hostnames, so the comparator refuses to label
network drift provider-induced without hostname/DNS binding.

1. **Task-induced drift** — both runtimes touched the same
   surface element to satisfy the contract. This is the "good"
   case: drift is zero where the contract pinned it down.
2. **Provider-induced drift** — non-shared items match a
   provider-host whitelist (`api.openai.com`,
   `generativelanguage.googleapis.com`, ...). This label is
   supported by the comparator and synthetic fixture; the live v0
   baseline does not carry enough hostname data to use it.
3. **Runtime-induced drift** — non-shared items not on the
   provider whitelist and not in the fixture-path whitelist.
   Attributable to the runtime's loader, sidecar machinery, or
   vendored deps.
4. **Inconclusive drift** — one arm has zero data and the other
   has some; cannot mechanically tell whether the dimension is
   genuinely empty in that arm or just not measured.

These four labels are the report vocabulary. The live claim is narrower:
across three real archive pairs, filesystem and network rows landed
`runtime-induced`, SDK tool surface and invocation order landed
`task-induced`, and process/MCP rows landed `inconclusive`.

What this post does **not** show:

- This post does not declare a winner between `@openai/agents`
  and `@google/genai`. The drift report is descriptive.
- This post does not claim the provider-host whitelist is
  exhaustive — new providers need a `--provider-host` override.
- This post does not say more than the v0 `capability_surface`
  schema can supply. Operation-aware file rows use optional
  `layers/kernel.ndjson` open metadata; unlink/remove and per-path
  access counts remain out of scope.

## The setup

One **workload contract**
([`WORKLOAD_CONTRACT.md`](https://github.com/Rul1an/assay/blob/main/docs/experiments/cross-runtime-drift-2026-05/WORKLOAD_CONTRACT.md))
that both runtime implementations must satisfy:

- Two tools, exactly: `read_file(path)` and
  `write_file(path, contents)`.
- One prompt, exactly: "read INPUT, uppercase the contents,
  write to OUTPUT, then reply `DONE`."
- One required tool-call sequence: `read_file → write_file`.
- One contract-checker that validates outputs after each run.

Two implementations:

- [`workload-openai/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/cross-runtime-drift-2026-05/workload-openai) —
  `@openai/agents@0.11.4`, standard agent loop, `temperature: 0`.
- [`workload-gemini/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/cross-runtime-drift-2026-05/workload-gemini) —
  `@google/genai@2.6.0`, **manual function-calling loop**
  (`automaticFunctionCalling.disable = true`), our own dispatch.
  The auto-vs-manual asymmetry on the Gemini side is part of
  what we measure; it is not smuggled in.

Both implementations write `tool-calls.ndjson` + `run-meta.json`
into their work directory. The contract-checker validates these
without needing the Runner archive — that's the Slice 1
deliverable. The Slice 3 dispatch workflow runs the
contract-checker per iteration *before* uploading any artefact,
so a contract-violating run never ships as a baseline.

Two arms run under one capture boundary
(`assay runner-spike` + cgroup v2 + eBPF on the same
`assay-bpf-runner` host). The kernel-level boundary is the same
for both arms by construction; whatever drifts is the
*runtime's* contribution, not the kernel's.

## The drift dimensions

The comparator looks at seven dimensions, each pinned to an
exact archive source so it cannot silently mix layers:

| Dimension | Source | What "drift" looks like |
|---|---|---|
| `filesystem_paths_touched` | `capability_surface.filesystem_paths` | Set diff of paths touched (undifferentiated under v0) |
| `kernel_file_operations` | `layers/kernel.ndjson` open metadata | Set diff of `operation:path` strings for successful opens |
| `network_endpoints` | `capability_surface.network_endpoints` | Set diff of `host:port` strings |
| `process_execs` | `capability_surface.process_execs` / `layers/kernel.ndjson` | Set diff of exec targets |
| `sdk_tool_events` | `layers/sdk.ndjson` | Set diff of SDK-emitted tool names |
| `mcp_tool_surface` | `capability_surface.mcp_tools` | Set diff of MCP server/client/tool names |
| `tool_invocation_order` | `layers/sdk.ndjson` (seq-ordered) | Per-`tool_call_id` ordering diff |

Three things are deliberately out of scope for the v0
comparator:

- Unlink/remove and fd-level byte-count semantics. The committed
  baseline can split successful open events into
  `read`/`write`/`create`/`truncate`/`append` using kernel open
  metadata, but it does not claim full filesystem semantics.
- Per-path access counts.
- Latency, token cost, model output quality.

These are tracked alongside the
[`runner-vs-otel-2026-05`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-2026-05/v1-findings.md#what-still-does-not-prove)
"What still does NOT prove" list — both experiments share the
same v0 boundary.

## The classifier

For each dimension, the comparator emits one row with:

- The set of items only in arm A (`only_in_a`)
- The set of items only in arm B (`only_in_b`)
- The set of items in both (`in_both`)
- One of four classification labels — task-induced /
  provider-induced / runtime-induced / inconclusive
- A short detail string explaining the label

The classification rules are conservative on purpose:

```
if both arms empty                          → inconclusive
if one arm empty, other not                 → inconclusive  (can't attribute)
if non-shared set empty (full overlap)      → task-induced
if all non-shared items match providers     → provider-induced  (network-dim only)
if all non-shared items match fixture paths → task-induced
otherwise                                   → runtime-induced
```

Provider-host matching is **exact-or-subdomain**, never
substring. A path-shaped string that *contains* `api.openai.com`
(say, a cache filename) will not get misclassified as
provider-induced — the substring trap is exactly the bug a
careful reviewer caught in an earlier draft. The
`NetworkEndpointParsingTests` cases in `test_drift.py` pin this
invariant.

Provider-host classification is also dimension-gated: only the
`network_endpoints` row passes `is_network_dimension=True`.
Other dimensions never run the provider check at all. Same
reasoning — a filesystem path that happens to contain a provider
hostname is not a provider transport, it's a filename.

## What the live n=3 run shows

| Dimension | Classification | Why |
|---|---|---|
| `filesystem_paths_touched` | **runtime-induced** | Each arm touches three arm-local/runtime-specific paths (`sdk-events.ndjson`, `tool-calls.ndjson`, and that workload's `dist/package.json`) plus shared host resolver config. |
| `network_endpoints` | **runtime-induced** under v0 | Live capture records IP endpoints rather than provider hostnames. Non-shared OpenAI/Gemini IPs cannot be matched to the provider-host whitelist, so the comparator refuses to guess. |
| `process_execs` | **inconclusive** | Empty in both arms under capability_surface v0; the captured Node workload is not represented as a child exec. |
| `sdk_tool_events` | **task-induced** | Both arms emit SDK events for `read_file` + `write_file`. |
| `mcp_tool_surface` | **inconclusive** | Empty in both arms (the workload contract forbids MCP). Expected-inconclusive, not surprising-inconclusive. |
| `tool_invocation_order` | **task-induced** | Both arms invoke `read_file → write_file` in that order. Contract enforces it. |

Two task-induced, two runtime-induced, two inconclusive — stable across
all three live pairs.

The most interesting row changed from what the synthetic fixture
predicted. I expected network drift to classify provider-induced. In
the live data, it does not, because v0 `network_endpoints` stores IPs:
Cloudflare/OpenAI-side IPs, Google-side IPs, and shared local resolver
traffic at `127.0.0.53:53`. Without hostname/DNS evidence in the
archive, assigning those IPs to providers would be an inference outside
the contract. The right result is to say "v0 cannot make that provider
claim."

## Why the four labels matter

The labels are useful exactly because they separate concerns:

- **task-induced** dimensions are the contract surface — if
  they drift across runtimes, your contract is broken or your
  comparator is. Both are interesting; neither is the runtime's
  fault.
- **provider-induced** dimensions describe the provider's
  transport. They will look different across providers. They
  will look different even within the same provider as new
  endpoints come online. They are not a runtime quality signal.
- **runtime-induced** dimensions are the publishable signal.
  This is what you actually wanted to measure when you asked
  "what does runtime choice cost me at the surface level."
- **inconclusive** dimensions are honesty. We refuse to label
  empty-in-one-arm dimensions as "task-induced" by default;
  the row carries `inconclusive` and the findings doc explains
  every such row by hand or downgrades it to a known limitation.

A run that produces *only* task-induced rows is uninteresting —
the runtimes are indistinguishable under your contract. A run
that produces only runtime-induced rows means you're comparing
two very different stacks. Most real runs sit in the middle,
which is the whole point.

## Reproducing the run

```bash
git clone https://github.com/Rul1an/assay
cd assay
export REPO_ROOT="$PWD"

# Comparator + helpers (53 unit tests, no API keys required).
python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare" \
  -p 'test_*.py'

# Smoke run against the synthetic fixtures (exercises all classifier labels).
python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/drift.py" \
  --archive-a "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-a-openai" \
  --archive-b "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-b-gemini" \
  --fixture-path /tmp/work/fixture-input.txt \
  --fixture-path /tmp/work/fixture-output.txt \
  --path-alias /tmp/work/fixture-input.txt=workdir/input \
  --path-alias /tmp/work/fixture-output.txt=workdir/output \
  --out-md /tmp/drift.md
cat /tmp/drift.md

# Verify the committed live archives' health.
for archive in \
  "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/a0/*/archive.tar.gz \
  "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/b0/*/archive.tar.gz
do
  python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/health_gate.py" \
    --archive "$archive"
done
```

If you have OpenAI + Gemini keys and a Linux host with cgroup
v2, you can run the workloads locally:

```bash
cd "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/workload-openai"
npm install --ignore-scripts && npx tsc -p tsconfig.json
WORKLOAD_WORK_DIR=$(mktemp -d) OPENAI_API_KEY=... node dist/workload.js

cd "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/workload-gemini"
npm install --ignore-scripts && npx tsc -p tsconfig.json
WORKLOAD_WORK_DIR=$(mktemp -d) GOOGLE_API_KEY=... node dist/workload.js
```

Then run the contract-checker on each work directory; both should exit
0. To get a fresh Runner archive (and therefore a fresh drift report),
you need `assay runner-spike run` on a Linux/eBPF host — the
[`Cross-Runtime Drift Experiment`](https://github.com/Rul1an/assay/blob/main/.github/workflows/cross-runtime-drift-experiment.yml)
workflow on `assay-bpf-runner` does this end-to-end and uploads
both arm archives + the per-pair drift reports as artefacts. The
committed baseline in this repo comes from run
[`26398427430`](https://github.com/Rul1an/assay/actions/runs/26398427430).

## Relationship to the OpenInference vocabulary question

Earlier in the year I filed
[`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162)
asking how an OTel/OpenInference trace should refer to a
content-addressed runtime-evidence artifact captured alongside
the same agent run. The four candidate attributes there
(`agent.runtime_evidence.{digest, health, boundary,
intent_effect_status}`) are runtime-agnostic by design.

Cross-runtime drift is **evidence** that motivates those
attributes more strongly: if `runtime_evidence.boundary` and
`runtime_evidence.health` describe what a single Runner archive
measured, the comparator described here describes what *changes*
when you swap the runtime under the same boundary. The two
experiments together describe the joint: per-run evidence
identity + cross-run drift shape.

The vocabulary question this experiment surfaces — should
trace-level metadata describe the *classification* of an
observed surface drift, or is that strictly downstream comparator
territory? — is deliberately *not* a separate OpenInference
issue today. It stays as a comment on #3162 if the maintainers
ask, and otherwise stays on disk in the publication folder.

## Non-claims (in case anyone reading this thinks I'm selling something)

- This is not a benchmark. n=3 is shape stability, not perf.
- This is not a runtime ranking. The drift report describes
  what differs; it does not say which arm "wins."
- This is not a security claim. "Runtime B contacts an extra
  host" is a runtime-selection input, not "runtime B is
  insecure."
- This is not a proposal for OpenInference / OTel to adopt
  Assay-Runner. The Runner is internal to the assay project,
  Linux-only, with no third-party users.
- This is not a paper. There is no academic claim. It is a
  reproducible engineering write-up against on-disk evidence.

## Next

The data is now live, but the publication discipline has not changed:
wait for OpenInference #3162 triage before posting this broadly. If a
maintainer asks for a concrete cross-runtime example, point at the
committed `runs/` package and keep the vocabulary question narrow.
