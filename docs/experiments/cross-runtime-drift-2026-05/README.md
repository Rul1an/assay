# cross-runtime-drift-2026-05

> **Status:** Slices 1 and 2 landed. Workload contract written, two
> runtime implementations runnable locally with API keys,
> contract-checker validates outputs (13 stdlib unit tests), and the
> `compare/drift.py` MVP produces a per-dimension drift report
> against the synthetic fixtures with 16 stdlib unit tests passing.
> Slices 3–5 still TODO (live captures on `assay-bpf-runner`,
> findings doc, publication artefacts).
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
| [`compare/`](compare/) | Slice 2: `drift.py` stdlib comparator + `test_drift.py` (16 tests) + `fixtures/{arm-a-openai,arm-b-gemini}/` synthetic archives that exercise every drift dimension. |

`runs/` will appear once Slice 3 dispatches live captures on
`assay-bpf-runner`. Not present yet.

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
- 13 contract-checker unit tests cover the happy path for both
  runtimes plus each individual rule failure mode plus malformed
  JSON / symlinked workdir handling.

## What's done in Slice 2

- `compare/drift.py` MVP: stdlib Python comparator that reads two
  Runner archives (directories or `.tar.gz`) and emits a
  per-dimension drift report. Dimensions are pinned to v0
  capability_surface sources (no read/write/create/remove split
  — that's an explicit v2 follow-up tracked in the plan-doc).
- `compare/fixtures/arm-a-openai/` and `arm-b-gemini/` synthetic
  archives wired to exercise every classification label exactly
  once: filesystem-paths `runtime-induced`, network-endpoints
  `provider-induced`, process-execs / SDK tools /
  tool-invocation-order `task-induced`, MCP `inconclusive`.
- 16 stdlib unit tests cover parsing (directory + tar.gz),
  failure modes (missing manifest, corrupt JSON, broken tar),
  every classification label, fixture-path overrides, and CLI
  output (`--out-json`, `--out-md`).
- Output schema locked in: `assay.cross_runtime_drift.v0`.

## What's deliberately NOT in Slice 1 or Slice 2

- No Runner capture wiring. That arrives in Slice 3 alongside an
  `assay-bpf-runner` workflow extension.
- No `GOOGLE_API_KEY` workflow secret. Local-only until Slice 3.
- No live n=3 baselines. Comparator runs against synthetic
  fixtures only; live data is Slice 3.
- No OTel trace emission. The cross-runtime comparison is
  between two Runner archives; traces are an explicit follow-up.
- No read/write/create/remove split, no per-path access counts,
  no kernel-ndjson parsing. All tracked as deferred v2 follow-ups
  in the plan-doc.
