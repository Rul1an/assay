# cross-runtime-drift-2026-05

> **Status:** Slice 1 landed. Workload contract written, two runtime
> implementations runnable locally with API keys, contract-checker
> validates outputs with 10 stdlib unit tests passing. Slices 2–5
> still TODO.
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
- 10 contract-checker unit tests cover the happy path for both
  runtimes plus each individual rule failure mode.

## What's deliberately NOT in Slice 1

- No Runner capture wiring. That arrives in Slice 3 alongside an
  `assay-bpf-runner` workflow extension.
- No `compare/drift.py` yet. That is Slice 2 (works against
  synthetic archives first to lock the output schema).
- No CI dispatch. `GOOGLE_API_KEY` is local-only until the
  workload contract is reviewed and Slice 2's comparator schema
  is stable.
- No OTel trace emission. The cross-runtime comparison is
  between two Runner archives; traces are an explicit follow-up.
