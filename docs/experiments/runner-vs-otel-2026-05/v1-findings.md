# v1 Findings: Runner Archives Next to OTel Traces

> **Status:** Arm B (trace-only) baseline complete on macOS Node 22 local
> environment. Arms A and C remain pending on the delegated `assay-bpf-runner`
> host. This document captures the v1 evidence that the experiment design and
> tooling actually work end-to-end; substantive cross-arm findings land here
> when Arm C dispatches succeed.
>
> **Plan:** [../runner-vs-otel-shape-comparison-2026-05.md](../runner-vs-otel-shape-comparison-2026-05.md)
>
> **Date of v1 baseline runs:** 2026-05-24
>
> **Reproducibility:**
> - Node `v22.16.0` via nvm
> - `@openai/agents@0.11.4`, `@opentelemetry/api@^1.9.0`,
>   `@opentelemetry/sdk-trace-base@^2.0.0`,
>   `@opentelemetry/resources@^2.0.0`
> - Python `3.14.3` (stdlib only for `compare.py`)
> - macOS / Apple Silicon (Linux for L2 capture; out of v1 scope here)

## What the v1 baseline runs prove

All four claims below are backed by artifacts under
[`runs/v1-baseline/`](runs/v1-baseline/). Each run includes the raw
`trace.json`, the comparator's `matrix.json`, and the human-readable
`matrix.md`.

### 1. Arm B workload produces a non-empty, schema-conforming OTel trace

Three identical Arm B runs were performed back-to-back. Each produced a
two-span trace with the same span shape:

| Run | Spans | Tool name | `gen_ai.tool.call.id` | `gen_ai.provider.name` |
|---|---:|---|---|---|
| `run_arm_b_20260524T193930Z` | 2 | `read_file` | `tc_runner_policy_001` | `openai` |
| `run_arm_b_20260524T194017Z_2` | 2 | `read_file` | `tc_runner_policy_001` | `openai` |
| `run_arm_b_20260524T194019Z_3` | 2 | `read_file` | `tc_runner_policy_001` | `openai` |

The trace shape is stable across runs. This satisfies the
*Trace shape stability* row of the Measurement Plan
(`n = 3`, soft gate).

### 2. `gen_ai.tool.call.id` join works against a fixture archive

For all three Arm B runs, the comparator reported:

```
tool_call_id join: joined:tc_runner_policy_001
```

That is, the trace's `gen_ai.tool.call.id` attribute matched the fixture
archive's `sdk_event.tool_call_id`, populating the primary join key
without falling back to weaker keys (tool name + monotonic order, or
timestamp proximity).

This is the v1 evidence for the *Join Hierarchy* table's
**Direct manual OTel SDK** row: with manual SDK instrumentation,
`gen_ai.tool.call.id` is present by construction and the join grade is
**Primary**.

### 3. Manifest-digest binding works end-to-end as a tamper-evident link

A fourth run was performed with `--archive` pointing at the synthetic
fixture archive. The workload computed the SHA-256 of the archive's
`manifest.json` bytes via `manifest-binding.ts` and attached an
`assay.archive.created` span event with these attributes:

```
assay.archive.schema           = "assay.runner.archive_manifest.v0"
assay.archive.manifest_digest  = "sha256:c76eb655e4630235ad137a50427a47db4b70ab9dcb40ddf30ad3f3165ee9d1d8"
assay.archive.path             = "<fixture archive path>"
assay.archive.manifest_bytes   = 261
assay.archive.source           = "directory"
```

The comparator then verified the trace-side digest against the
archive-side digest:

```
manifest-digest binding: tamper-evident-match
```

Running the same comparator with `--require-binding-match` returned
**exit code 0**. The earlier non-matching test fixture (different
`assay.archive.manifest_digest` value) correctly produced
**exit code 3** with the strict flag, confirming the contract.

This is the v1 evidence for the **Manifest Digest Binding** section of
the plan doc: a span event can refer to an archive by digest without
embedding archive content, and the comparator can verify the binding
deterministically.

### 4. Asymmetric coverage matches the design hypothesis

The Arm B field matrix exhibits the expected asymmetry between L1
(trace) and L2 (archive) evidence. Fields are summarized as
**L1 present / L2 present / both / asymmetric**:

| Field class | Arm B (trace-only) observation |
|---|---|
| Reported control flow (`gen_ai.provider.name`, `gen_ai.tool.name`, `gen_ai.tool.call.id`) | **L1 only** |
| Reported provenance (`gen_ai.request.model`, `gen_ai.response.model`) | **L1 absent** in the deterministic workload (the cassette model does not populate these); would be **L1 present** with a real model call. |
| Reported usage (`gen_ai.usage.input_tokens`, `output_tokens`) | **L1 absent** for the same reason. |
| Measured system effects (filesystem paths, network endpoints, process execs) | **L2 only** (taken from the fixture archive; Arm C will produce real values) |
| Measurement integrity (`ringbuf_drops`, `cgroup_correlation`) | **L2 only**; Arm B has nothing to report |
| Primary join key (`tool_call_id`) | **Both, joined** |
| Tamper-evident binding (`manifest_digest`) | **Both, joined** (dual-simulation run) |
| Run identity (`run_id`) | **L1 present**, **L2 from fixture**: trace has the real Arm B run id, archive has the fixture's run id; these differ by design in this control arm. |

The matrix-row count is exactly 16; the markdown render passes the
smoke test in `tests/test_compare.py::test_markdown_renders`.

## What v1 does NOT prove

These claims wait for Arms A and C to be dispatched on the
`assay-bpf-runner` host:

- Linux/eBPF/cgroup-v2 actually populates `capability_surface.*` fields
  on real workloads;
- `observation_health.ringbuf_drops == 0` holds for the dual-capture
  workload under the experiment's instrumentation overhead;
- `correlation_report.status == clean` holds when both Runner and OTel
  SDK instrumentation are active in the same process;
- Per-run archive determinism: byte-identical `manifest.json` across
  three Arm C runs of the same workload (a hard gate in the plan doc);
- Overhead measurements (`n >= 20` wall clock, `n >= 5` RSS).

Until those land, v1 stands only as evidence that the **measurement
infrastructure** is correct. The published claim of the experiment is
gated on Arm C data.

## Reproduction commands

```bash
# Install deps and build the workload once
export PATH="$HOME/.nvm/versions/node/v22.16.0/bin:$PATH"
cd docs/experiments/runner-vs-otel-2026-05/workload
npm install --no-audit --no-fund --ignore-scripts
npx tsc -p tsconfig.json

# Arm B: trace-only run
RUN_ID="run_arm_b_$(date -u +%Y%m%dT%H%M%SZ)"
RUN_DIR="../runs/$RUN_ID"
mkdir -p "$RUN_DIR"
node dist/workload.js --run-id "$RUN_ID" --trace-out "$RUN_DIR/trace.json"

# Compare against the synthetic fixture archive (Arm B has no real archive)
cd ../..
python3 docs/experiments/runner-vs-otel-2026-05/compare/compare.py \
  --archive docs/experiments/runner-vs-otel-2026-05/compare/tests/fixtures/archive \
  --trace docs/experiments/runner-vs-otel-2026-05/runs/$RUN_ID/trace.json \
  --out-json docs/experiments/runner-vs-otel-2026-05/runs/$RUN_ID/matrix.json \
  --out-md docs/experiments/runner-vs-otel-2026-05/runs/$RUN_ID/matrix.md
```

## Pinned versions for v1 baseline

```text
Node:                 v22.16.0
Python:               3.14.3 (Homebrew, Apple Silicon)
@openai/agents:       0.11.4
@opentelemetry/api:   ^1.9.0 (resolved 1.9.x at install time)
@opentelemetry/sdk-trace-base: ^2.0.0 (resolved 2.7.x)
@opentelemetry/resources:      ^2.0.0
zod:                  4.1.13
TypeScript:           ^5.5.0
```

`workload/package-lock.json` is intentionally git-ignored to avoid
churn on machine-specific install hashes; the version pins above are
the source of truth. A v1.1 follow-up that adds the OpenInference
instrumentation comparison row should pin its instrumentation package
versions here.

## Non-claims

- This v1 baseline does not assert that measured-run archives are
  better than traces, nor that OTel cannot carry runtime-evidence
  attributes. It asserts only that the comparator, the workload, and
  the manifest-digest binding work as specified.
- No prompt content, no completion content, and no tool argument or
  result was captured in these runs. Sensitive content remains
  off-by-default per the `--capture-sensitive-otel-content` policy.
- The fixture archive used in Arm B is synthetic and is not evidence
  of any agent's actual filesystem behavior. Real `capability_surface`
  data lands with Arm C.
