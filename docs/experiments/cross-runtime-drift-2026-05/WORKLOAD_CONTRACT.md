# Workload Contract — cross-runtime-drift-2026-05

> **Purpose:** make "same agent task" *checkable*, not aspirational.
> Both runtime implementations of the workload must satisfy every
> rule below. The `contract-checker/check.py` script validates rules
> 1–6 automatically after a run; rules 7–8 are pinned design
> decisions enforced by the workload code itself.
>
> **Status:** Slice 1 of the
> [`cross-runtime-drift-2026-05`](../cross-runtime-drift-2026-05.md)
> experiment. Resolves Open Questions #1 and #3.

## Open Question resolutions baked into this contract

- **Q1 — Second runtime:** Google Gemini via `@google/genai`.
  Reason: closest semantic parity with `@openai/agents`,
  single-vendor stack so we are measuring runtime drift not also
  framework drift.
- **Q1 sub — Tool-calling mode:** Gemini side uses
  `automaticFunctionCalling: { disable: true }` and orchestrates
  the loop in *our* code. OpenAI side uses `@openai/agents` in
  its standard agent-loop mode. The auto-vs-manual asymmetry
  **is** part of the runtime drift we want to surface; it is not
  smuggled in.
- **Q3 — Determinism:** real model calls with `temperature: 0` +
  a tight prompt + minimum required tools. We deliberately do
  *not* use a cassette/fake model here — the whole point is to
  measure each runtime's actual transport, loader, and SDK
  machinery against the kernel. Variance in the model's
  *exact* string output is tolerated; specifically, one terminal
  newline on the uppercased file contents is not a drift dimension.
  Variance in the
  *tool-call sequence* is a contract violation.

## Inputs

The host (or CI workflow) must provide:

| Env var | Required | Meaning |
|---|---|---|
| `WORKLOAD_WORK_DIR` | yes | Absolute path to an empty directory the workload owns for this run |
| `WORKLOAD_INPUT_PATH` | no, default `$WORKLOAD_WORK_DIR/fixture-input.txt` | File the workload's read tool will be pointed at |
| `WORKLOAD_OUTPUT_PATH` | no, default `$WORKLOAD_WORK_DIR/fixture-output.txt` | File the workload's write tool must produce |
| `WORKLOAD_INPUT_CONTENTS` | no, default `cross-runtime drift fixture\n` | Contents written to `WORKLOAD_INPUT_PATH` before the agent runs |
| `OPENAI_API_KEY` | only for `workload-openai` | OpenAI auth |
| `GOOGLE_API_KEY` | only for `workload-gemini` | Gemini auth |

The workload **creates `WORKLOAD_INPUT_PATH` itself** with
`WORKLOAD_INPUT_CONTENTS` before invoking the agent. The
contract-checker assumes this.

## Tools the workload must register

Both implementations register **exactly these two tools, with
these names and these signatures**:

```
read_file(path: string) -> string
  Returns the UTF-8 contents of `path`.
  Errors if path is outside WORKLOAD_WORK_DIR or does not exist.

write_file(path: string, contents: string) -> void
  Writes `contents` (UTF-8) to `path`.
  Errors if path is outside WORKLOAD_WORK_DIR.
```

No other tools, no MCP servers, no extra helpers. Any deviation
is a workload bug, not a drift signal.

## The prompt

Both implementations send **exactly the same user prompt**:

> Read the file at `${WORKLOAD_INPUT_PATH}`, uppercase its
> contents, then write the result to `${WORKLOAD_OUTPUT_PATH}`.
> Call `read_file` first and `write_file` second. Do not call
> any other tool. When done, reply with the single word `DONE`.

`WORKLOAD_INPUT_PATH` and `WORKLOAD_OUTPUT_PATH` are interpolated
into the prompt as absolute paths before sending.

System / instruction message (also identical):

> You are a deterministic agent. Use the provided tools to do
> exactly what the user asked. Do not paraphrase the task. Do
> not add commentary. Reply with the literal word `DONE` when
> the work is complete.

## Required tool-call sequence

The agent **must** invoke tools in this order:

1. `read_file(path = WORKLOAD_INPUT_PATH)`
2. `write_file(path = WORKLOAD_OUTPUT_PATH, contents = <uppercase of step-1 result>)`

The checker treats one terminal newline as insignificant for the
uppercased contents. This keeps model style variance ("same text,
missing final line terminator") from blocking a runtime-drift
capture; any other content difference remains a contract failure.

Any other order, any other tool, any repeated invocation of
either tool, or any extra invocations counts as a contract
violation. The contract-checker enforces this by reading a
`tool-calls.ndjson` file the workload writes (see Outputs).

## Outputs

Each workload run must produce, in `$WORKLOAD_WORK_DIR`:

| File | Required | Format | Used by |
|---|---|---|---|
| `fixture-input.txt` (or override) | yes | Pre-seeded by the workload | the read tool |
| `fixture-output.txt` (or override) | yes | Written by the write tool | contract-checker |
| `tool-calls.ndjson` | yes | One JSON object per line: `{"seq": int, "tool": string, "args": {...}}` | contract-checker |
| `run-meta.json` | yes | `{"runtime": "openai-agents" \| "gemini-genai", "model": "...", "sdk_version": "...", "started_at": "...", "ended_at": "...", "exit_code": 0}` | contract-checker + future drift reports |

`tool-calls.ndjson` is written by the workload itself
(instrumented inside the tool handlers), *not* derived from
Runner archives. Contract conformance is independent of whether
the run was captured under Runner.

## Exit codes

| Code | Meaning |
|---|---|
| 0 | Workload completed and (the workload believes) satisfied the contract |
| 1 | Internal workload error (failed to bootstrap, SDK error, etc.) |
| 2 | Workload self-detected a contract violation. Fail-fast cases: agent called `read_file` with `path != WORKLOAD_INPUT_PATH`, `write_file` with `path != WORKLOAD_OUTPUT_PATH`, a path outside `WORKLOAD_WORK_DIR`, or an unregistered tool. Both implementations throw a `ContractViolationError` from the tool handler so the side effect never lands on disk. |
| 3 | Model output rejected (no `DONE` produced within the allowed iterations) |

Contract-checker independently re-verifies; the workload's exit
code is advisory, not authoritative.

## Validation rules (automatically enforced by `contract-checker/check.py`)

Given `$WORKLOAD_WORK_DIR`, the checker confirms:

1. `fixture-output.txt` exists and is non-empty.
2. `fixture-output.txt` equals `WORKLOAD_INPUT_CONTENTS` uppercased
   modulo one terminal newline.
3. `tool-calls.ndjson` exists, has exactly two lines.
4. Line 1: `tool == "read_file"`, `args.path == WORKLOAD_INPUT_PATH`.
5. Line 2: `tool == "write_file"`, `args.path == WORKLOAD_OUTPUT_PATH`,
   `args.contents == WORKLOAD_INPUT_CONTENTS.upper()` modulo one terminal
   newline.
6. `run-meta.json` exists and parses; `exit_code == 0`; `runtime`
   matches one of the two allowed values.
7. (Design rule, not auto-checked) Both implementations use the
   tool-calling mode pinned in this contract: `@openai/agents`
   standard agent loop on the OpenAI side, `@google/genai` with
   `automaticFunctionCalling.disable = true` on the Gemini side.
8. (Design rule, not auto-checked) No additional tools, MCP
   servers, or external services are invoked from the workload
   code. Whatever extra surface appears in a Runner capture is
   runtime-induced, not workload-induced.

## What this contract does NOT specify

- **Latency or token counts.** Out of scope for the drift
  experiment. Will be measured separately if at all.
- **Exact model version.** Both workloads pin a model in
  `run-meta.json`, but the contract does not require both to use
  the same family. The plan-doc's "single snapshot in time" caveat
  applies.
- **Exact text of the model's terminating reply** beyond requiring
  the literal `DONE`. Models may add or omit punctuation; the
  contract accepts `DONE`, `DONE.`, `DONE!`, etc.
- **OTel trace emission.** The runner-vs-otel-2026-05 experiment
  proved binding + tool-level join with OTel traces. For
  cross-runtime-drift the comparison is between two Runner
  archives, not traces. Traces may be added later; they are not
  part of Slice 1.
