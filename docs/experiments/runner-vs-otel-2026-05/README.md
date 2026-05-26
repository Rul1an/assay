# Runner vs OTel: Shape Comparison Experiment Package

> **Status:** v1 evidence and Slice 2-4 follow-ups landed on `main`.
> Arm B trace-only runs, Arm C delegated Linux/eBPF dual-capture runs,
> SDK-layer ingestion, tool-level L1/L2 join, and the controlled
> tool-call argument tampering scenario all have committed evidence
> under [`runs/`](runs/). The tampering scenario also has a
> kernel-event-v0 rerun with operation-aware open metadata. Publication
> drafts live under
> [`publication/`](publication/) and remain gated on OpenInference
> [#3162](https://github.com/Arize-ai/openinference/issues/3162)
> triage.
>
> **Plan doc:** [../runner-vs-otel-shape-comparison-2026-05.md](../runner-vs-otel-shape-comparison-2026-05.md)
> — read this first for the framing, hypotheses, claim-class taxonomy, and
> threats to validity.

## What is in this directory

| Path | Purpose |
|---|---|
| `compare/compare.py` | Stdlib-only Python comparator. Reads a Runner archive (`.tar.gz` or extracted dir) plus an OTLP/JSON trace and emits the field matrix as JSON and Markdown. |
| `compare/tests/test_compare.py` | Stdlib unit tests over synthetic fixtures. Includes the explicit-mismatch guard for the manifest-digest binding and the Slice 2/3 join + intent/effect cases. |
| `compare/tests/fixtures/` | Synthetic Runner archive directory tree and a matching OTLP trace JSON, used by the unit tests and by Arm B as a placeholder archive side. |
| `workload/` | Node.js + TypeScript workload that wraps the existing deterministic OpenAI Agents fixture (`runner-fixtures/openai-agents/fixture-agent.js`) with OpenTelemetry tracing. Produces one OTLP/JSON trace per run and, in dual-capture mode, attaches the `assay.archive.manifest_digest` event. |
| `run-arm-b.sh` | Local trace-only orchestrator (no eBPF required). |
| `runs/` | Committed per-run evidence for Arm B, Arm C, Slice 2, Slice 3, and the Slice 3 kernel-event-v0 rerun. |

## How to run each arm

### Arm A — Runner only (delegated host required)

Linux/eBPF/cgroup-v2; runs on `assay-bpf-runner`.

```bash
gh workflow run runner-spike-delegated.yml --ref main \
  -f gates=all -f build_ebpf=true
```

The existing delegated gate already produces a Runner archive at
`/tmp/assay-runner-proof-<run-id>/gates/openai-agents-kernel-policy/run-1/runner-openai-agents-kernel-policy.tar.gz`.
For the experiment, copy that tarball out of the delegated run artifacts and
feed it into the comparator with `--archive <tarball>`. No trace is captured
in Arm A; the matrix shows L1 columns as `absent`.

### Arm B — Trace only (local, macOS / Linux / Windows)

```bash
./run-arm-b.sh
```

Produces `runs/<run-id>/trace.json` plus a matrix that pairs the real trace
with the synthetic fixture archive. This arm exists to establish the trace
shape, the `gen_ai.tool.call.id` presence baseline (Direct manual OTel SDK
row in the join hierarchy table), and the overhead of the OTel SDK without
any eBPF tooling attached.

### Arm C — Dual capture (delegated host required)

Linux/eBPF; runs on `assay-bpf-runner` because the workload needs to be
invoked under `assay runner-spike run`. The workload script accepts an
`--archive <path>` flag so the trace's root span gets the
`assay.archive.created` event with the real `assay.archive.manifest_digest`
referring to the just-written archive.

Outline of the delegated invocation:

```bash
# inside the delegated host, after `cargo build -p assay-cli --features runner`
RUN_ID="run_dual_capture_$(date -u +%Y%m%dT%H%M%SZ)"
ARCHIVE_OUT="/tmp/assay-runner-otel-experiment/$RUN_ID/archive.tar.gz"

# 1. build the workload one-off
(cd docs/experiments/runner-vs-otel-2026-05/workload \
  && npm install --no-audit --no-fund --ignore-scripts \
  && npx tsc -p tsconfig.json)

# 2. run under runner-spike so the archive is produced; the workload itself
#    emits the OTLP trace and binds to the archive after writing.
target/debug/assay runner-spike run \
  --agent-shim openai-agents \
  --kernel-capture \
  --ebpf target/assay-ebpf.o \
  --run-id "$RUN_ID" \
  --output "$ARCHIVE_OUT" \
  -- node docs/experiments/runner-vs-otel-2026-05/workload/dist/workload.js \
     --run-id "$RUN_ID" \
     --archive "$ARCHIVE_OUT" \
     --trace-out "/tmp/assay-runner-otel-experiment/$RUN_ID/trace.json"

# 3. comparator
python3 docs/experiments/runner-vs-otel-2026-05/compare/compare.py \
  --archive "$ARCHIVE_OUT" \
  --trace "/tmp/assay-runner-otel-experiment/$RUN_ID/trace.json" \
  --out-json "/tmp/assay-runner-otel-experiment/$RUN_ID/matrix.json" \
  --out-md "/tmp/assay-runner-otel-experiment/$RUN_ID/matrix.md"
```

The committed Arm C baselines were produced through the workflow form of
this invocation and repeated three times. The shell outline above is kept
as a developer reference; [`v1-findings.md`](v1-findings.md) is the source
of truth for the actual run IDs, workflow runs, and evidence directories.

## Measurement plan execution checklist

Lifted from the plan doc, mapped to concrete commands:

| Metric | Sample size | Command |
|---|---:|---|
| Per-run manifest binding | n=3 | Done in Arm C + Slice 2/3; each trace binds to its own archive digest |
| Trace shape stability | n=3 | Done in Arm B and Arm C; see committed `trace.json` + `matrix.json` |
| `gen_ai.tool.call.id` presence | n=3 per path | Done for direct manual OTel SDK and Runner SDK event ingestion; other instrumentation paths remain optional follow-ups |
| End-to-end wall clock | n>=20 per arm | `hyperfine` over each arm (see `scripts/perf_e2e.sh`) |
| Peak RSS | n>=5 per arm | `/usr/bin/time -l` (macOS) or `/usr/bin/time -v` (Linux) |
| Archive size | n=3 | `stat` on `<archive>.tar.gz` |
| Trace export size | n=3 | `stat` on `trace.json` |

Emit wall-clock and RSS in Bencher Metric Format (`BMF_JSON=1` per
`docs/PERFORMANCE-ASSESSMENT.md`) so the overhead numbers slot into the
existing Criterion / Bencher baseline.

The follow-up measurement contract is tracked in
[`../runner-vs-otel-overhead-2026-05.md`](../runner-vs-otel-overhead-2026-05.md).
Do not publish overhead claims from the `n=3` evidence slices.

## Comparator tests

```bash
python3 -m unittest discover \
  -s docs/experiments/runner-vs-otel-2026-05/compare/tests \
  -p 'test_*.py'
```

Expected output:

```
----------------------------------------------------------------------
Ran 17 tests in 0.0xxs
OK
```

The unit tests run in stdlib Python; no virtualenv or extra packages needed.

## Reproducibility pins

| Source | Pin |
|---|---|
| OpenTelemetry GenAI semconv | Attribute names used by the workload: `gen_ai.provider.name`, `gen_ai.operation.name`, `gen_ai.tool.name`, `gen_ai.tool.call.id`, and opt-in `gen_ai.tool.call.arguments` |
| `@opentelemetry/api`, `sdk-trace-base`, `sdk-trace-node`, `resources`, `semantic-conventions` | See `workload/package-lock.json` |
| `@openai/agents` | `0.11.4` (pinned) |
| `assay` workspace | See per-slice workflow run links in `v1-findings.md` |
| `assay runner-spike` | Built from the workflow head commit recorded per slice |
| Kernel version | Delegated Arm C runs used `6.8.0-117-generic` on `assay-bpf-runner` |
| Python | Stdlib comparator; tests pass on local Python and workflow Python |

## What this experiment package does and does not do

**Does:** produces machine-readable evidence (trace, archive, matrix) for the
hypothesis test in the plan doc; pins the OTel GenAI attribute namespace; ties
the trace to the archive via a tamper-evident digest event; documents how to
reproduce on the delegated host.

**Does not:** evaluate model quality, rank tracing tools, claim semantic
equivalence between runtimes, or replace policy-acceptability evaluation. The
acceptability claim is explicitly outside the contract of both L1 and L2.
