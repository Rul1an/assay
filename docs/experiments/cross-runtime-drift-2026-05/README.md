# cross-runtime-drift-2026-05

> **Status:** Slices 1–5 are on disk, with live n=3 baselines
> committed from
> [GitHub Actions run 26398427430](https://github.com/Rul1an/assay/actions/runs/26398427430)
> on `assay-bpf-runner`. Workload contract written, two runtime
> implementations runnable locally with API keys, contract-checker
> validates outputs (14 stdlib unit tests), `compare/drift.py`
> produces per-dimension drift reports (51 stdlib unit tests covering
> `drift.py` + `health_gate.py` + `extract_fixture_paths.py`), live
> Arm A0/B0 archives are under [`runs/`](runs/), [`findings.md`](findings.md)
> reflects the live data, and [`publication/`](publication/) holds
> blog + discussion-comment drafts gated on OpenInference #3162 triage.
>
> **Plan-doc:** [`../cross-runtime-drift-2026-05.md`](../cross-runtime-drift-2026-05.md)
> (research question, drift dimensions, threats to validity, sequencing).
>
> **Companion experiment:** [`runner-vs-otel-2026-05/`](../runner-vs-otel-2026-05/)
> (where the Runner L2 capture + comparator pattern were first proven).

## Layout

| Path | Purpose |
|---|---|
| [`WORKLOAD_CONTRACT.md`](WORKLOAD_CONTRACT.md) | The rules every workload implementation must satisfy. Slice 1 deliverable. |
| [`workload-openai/`](workload-openai/) | `@openai/agents` implementation (standard agent loop). |
| [`workload-gemini/`](workload-gemini/) | `@google/genai` implementation (manual function-calling loop, `automaticFunctionCalling.disable = true`). |
| [`contract-checker/`](contract-checker/) | Stdlib-only Python validator. Independent of Runner capture. |
| [`compare/`](compare/) | Slice 2 + Slice 3 helpers: `drift.py` stdlib comparator, `health_gate.py`, `extract_fixture_paths.py`, 51 stdlib unit tests, and `fixtures/{arm-a-openai,arm-b-gemini}/` synthetic archives that exercise every drift classification label. |
| [`runs/`](runs/) | Slice 3 live Arm A0 + B0 baselines + per-pair drift reports from workflow run 26398427430. See [`runs/README.md`](runs/README.md). |
| [`findings.md`](findings.md) | Slice 4: live n=3 findings write-up plus threats to validity and reproduction commands. |
| [`kernel-v0-feasibility.md`](kernel-v0-feasibility.md) | Follow-up diagnostic: what `layers/kernel.ndjson` can support now that open metadata is present, and what still remains out of scope. |
| [`publication/`](publication/) | Slice 5: blog draft + discussion-comment draft, both gated on the OpenInference #3162 triage signal. Not filed, not published. |

## Running locally

Each workload is a self-contained Node 22+ TypeScript project. The
contract-checker is stdlib Python 3.10+.

All examples below assume `$REPO_ROOT` points at the assay repo
root, so each block can run independently of where the previous
one left the shell:

```bash
export REPO_ROOT="$(git rev-parse --show-toplevel)"
```

### workload-openai

```bash
cd "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/workload-openai"
npm install --no-audit --no-fund --ignore-scripts
npx tsc -p tsconfig.json

WORK=$(mktemp -d -t cross-runtime-openai)
WORKLOAD_WORK_DIR="$WORK" \
OPENAI_API_KEY="$OPENAI_API_KEY" \
  node dist/workload.js
```

### workload-gemini

```bash
cd "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/workload-gemini"
npm install --no-audit --no-fund --ignore-scripts
npx tsc -p tsconfig.json

WORK=$(mktemp -d -t cross-runtime-gemini)
WORKLOAD_WORK_DIR="$WORK" \
GOOGLE_API_KEY="$GOOGLE_API_KEY" \
  node dist/workload.js
```

### contract-checker

```bash
# Against the work-dir produced by either workload above. Runs from
# any cwd thanks to $REPO_ROOT.
python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/contract-checker/check.py" \
  --work-dir "$WORK"

# Tests (no API keys required):
python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/contract-checker" \
  -p 'test_*.py'
```

### compare/drift.py (Slice 2)

```bash
# Smoke run against the synthetic fixtures. Produces drift.json on
# stdout and drift.md if --out-md is given. No live runs required.
python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/drift.py" \
  --archive-a "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-a-openai" \
  --archive-b "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-b-gemini" \
  --fixture-path /tmp/work/fixture-input.txt \
  --fixture-path /tmp/work/fixture-output.txt \
  --out-md /tmp/drift.md

# Tests (no API keys required):
python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare" \
  -p 'test_*.py'
```

The comparator takes two Runner archives (directories or `.tar.gz`)
and emits per-dimension drift rows. Each row carries a
classification: `task-induced` / `provider-induced` /
`runtime-induced` / `inconclusive`. The classification is a
starting point; the findings doc (Slice 4) explains every
`inconclusive` row by hand or downgrades it to a known limitation.

## What's done in Slice 1

- Open Question #1 resolved: Gemini via `@google/genai`, manual
  function-calling loop on the Gemini side, `@openai/agents`
  standard agent loop on the OpenAI side. Asymmetry is part of
  the runtime drift we want to surface; the workload contract
  pins it explicitly.
- Open Question #3 resolved: real model calls (no cassette),
  `temperature: 0`, tight prompt, contract requires `read_file`
  then `write_file` and `DONE` reply. Variance in the model's
  exact terminating string is tolerated; variance in tool-call
  *sequence* is a contract violation.
- Both workloads emit `tool-calls.ndjson` and `run-meta.json`
  per the contract; checker validates without any Runner
  archive present.
- 14 contract-checker unit tests cover the happy path for both
  runtimes plus each individual rule failure mode plus malformed
  JSON / symlinked workdir handling, and the final-newline-insensitive
  content check required by the live OpenAI run.

## What's done in Slice 2

- `compare/drift.py` MVP: stdlib Python comparator that reads two
  Runner archives (directories or `.tar.gz`) and emits a
  per-dimension drift report. Dimensions are pinned to v0
  capability_surface plus optional `layers/kernel.ndjson` open
  metadata for operation-aware file rows.
- `compare/fixtures/arm-a-openai/` and `arm-b-gemini/` synthetic
  archives wired to exercise every classification label exactly
  once: filesystem-paths `runtime-induced`, network-endpoints
  `provider-induced`, process-execs / SDK tools /
  tool-invocation-order `task-induced`, MCP `inconclusive`.
- 17 stdlib unit tests cover parsing (directory + tar.gz),
  failure modes (missing manifest, corrupt JSON, broken tar),
  every classification label, fixture-path overrides, and CLI
  output (`--out-json`, `--out-md`).
- Output schema locked in: `assay.cross_runtime_drift.v0`.

## What's now done in Slice 3-5

- Slice 3 live workflow ran successfully on `assay-bpf-runner`:
  n=3 OpenAI captures, n=3 Gemini captures, and n=3 drift reports.
- All six archives passed the health gate:
  `ringbuf_drops=0`, `kernel_layer=complete`,
  `cgroup_correlation=clean`.
- Live findings are stable across all three pairs:
  filesystem, kernel-file-operation, and network rows classify
  `runtime-induced`; SDK tool events + invocation order classify
  `task-induced`; process and MCP rows classify `inconclusive`.
- Publication drafts now describe the live baseline, not a synthetic
  placeholder. They remain unpublished until #3162 triage gives a
  maintainer signal.

## What's deliberately NOT in this package

- No OTel trace emission. The cross-runtime comparison is
  between two Runner archives; traces are an explicit follow-up.
- No unlink/remove classification, no fd-level byte-count semantics,
  and no normalized logical task-path aliases for operation-aware rows.
  The follow-up note [`kernel-v0-feasibility.md`](kernel-v0-feasibility.md)
  records what the current kernel-event shape can and cannot support.
