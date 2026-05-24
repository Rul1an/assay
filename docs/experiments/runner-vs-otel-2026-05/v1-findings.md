# v1 Findings: Runner Archives Next to OTel Traces

> **Status:** Arm B baseline (macOS local, n=3 + dual-simulation) and Arm C
> baseline (delegated `assay-bpf-runner`, n=3 with real Linux/eBPF capture)
> are both landed. Arm A (Runner-only) is implicitly covered by the archive
> half of Arm C. v1.5 follow-ups (adversarial tool-call argument tampering,
> SDK-layer correlation fix, overhead measurement at n>=20) stay open.
>
> **Plan:** [../runner-vs-otel-shape-comparison-2026-05.md](../runner-vs-otel-shape-comparison-2026-05.md)
>
> **Date of v1 baseline runs:** 2026-05-24
>
> **Reproducibility (Arm B, macOS):**
> - Node `v22.16.0` via nvm
> - `@openai/agents@0.11.4`, `@opentelemetry/api@^1.9.0`,
>   `@opentelemetry/sdk-trace-base@^2.0.0`,
>   `@opentelemetry/resources@^2.0.0`
> - Python `3.14.3` (stdlib only for `compare.py`)
> - macOS / Apple Silicon
>
> **Reproducibility (Arm C, delegated):**
> - VM: Multipass Ubuntu 24.04.3 LTS, kernel `6.8.0-117-generic` (ARM64)
> - GitHub Actions self-hosted runner labelled `assay-bpf-runner`
> - Workflow: [`.github/workflows/runner-otel-experiment.yml`](../../../.github/workflows/runner-otel-experiment.yml)
>   dispatched against `main` with `repetitions=3`, `require_binding_match=true`,
>   `build_ebpf=true`
> - `cargo 1.94.0`, `rustc 1.94.0`, eBPF artifact built via `cargo xtask build-ebpf`

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

## What the Arm C (delegated, real eBPF) runs add

Three Arm C dual-capture iterations were performed on the
`assay-bpf-runner` self-hosted runner via
[`runner-otel-experiment.yml`](../../../.github/workflows/runner-otel-experiment.yml)
(GitHub Actions run `26372344619`, head `c6508780`,
`repetitions=3 require_binding_match=true build_ebpf=true`). Per-run
artifacts are committed under
[`runs/v1-arm-c/`](runs/v1-arm-c/) — each directory contains the
extracted `archive-contents/` (manifest, capability surface,
observation health, correlation report, ndjson layers), the
OTLP/JSON `trace.json`, and the comparator's `matrix.json` +
`matrix.md`. The raw `.tar.gz` tarballs are intentionally not
tracked; see [`runs/v1-arm-c/README.md`](runs/v1-arm-c/README.md)
for the tracking policy and the workflow-artifact link. All three
iterations passed `--require-binding-match` (exit code 0).

### 5. Real L2 capture: eBPF observes Node.js file I/O the trace cannot see

For each Arm C run, the archive's `capability_surface.filesystem_paths`
records the *measured* file paths the kernel saw the Node.js workload
open. These are not paths the OTel trace ever reports:

```
/opt/actions-runner/_work/assay/assay/docs/experiments/runner-vs-otel-2026-05/workload/dist/workload.js
/opt/actions-runner/_work/assay/assay/docs/experiments/runner-vs-otel-2026-05/workload/dist/otel-setup.js
/opt/actions-runner/_work/assay/assay/docs/experiments/runner-vs-otel-2026-05/workload/dist/manifest-binding.js
/opt/actions-runner/_work/assay/assay/docs/experiments/runner-vs-otel-2026-05/workload/dist/package.json
/opt/actions-runner/_work/assay/assay/docs/experiments/runner-vs-otel-2026-05/workload/package.json
/opt/actions-runner/_work/assay/assay/arm-c-runs/<run-id>/workdir/openai-agents-input.txt
/opt/actions-runner/_work/assay/assay/arm-c-runs/<run-id>/trace.json
```

L1 trace shows: `gen_ai.tool.name = read_file`,
`gen_ai.tool.call.id = tc_runner_policy_001`, `gen_ai.provider.name = openai`.
**It says nothing about the JavaScript module graph the Node runtime
actually loaded, the `package.json` reads, or the trace-export write.**
The archive records exactly those.

This is the **measurement-effect asymmetry the experiment was
designed to prove**, now on real Linux/eBPF data rather than a synthetic
fixture.

### 6. Measurement-health hard gates hold under real capture

For all three Arm C runs:

| Field | Value | Source |
|---|---|---|
| `observation_health.kernel_layer` | `complete` | per-archive `observation-health.json` |
| `observation_health.ringbuf_drops` | `0` | same |
| `observation_health.cgroup_correlation` | `clean` | same |
| `correlation_report.status` | `clean` | per-archive `correlation-report.json` |
| `events.ndjson` line count | `2` (stable across runs) | archive contents |
| `layers/kernel.ndjson` line count | `7` (stable across runs) | archive contents |

The hard gate `ringbuf_drops == 0` was satisfied for all three runs.
This is the first time we have empirical confirmation that the
experiment's instrumentation footprint (OTel SDK + workload + Node
runtime) does not push the eBPF capture into ring-buffer drop
territory on this delegated host.

### 7. Tamper-evident binding works on real (not synthetic) archive bytes

For each Arm C iteration, `compare/bind-archive.py` (run as a workflow
post-step) computed the SHA-256 of the **exact bytes** of
`manifest.json` from the `assay runner-spike`-produced `.tar.gz` and
injected an `assay.archive.created` event onto the root span. The
comparator then verified the trace-side digest against the
archive-side digest under `--require-binding-match`:

| Run | Manifest digest | Binding |
|---|---|---|
| `run_arm_c_20260524T205016Z_1` | `sha256:fe913819…dcd600` | `tamper-evident-match` |
| `run_arm_c_20260524T205018Z_2` | `sha256:f42f2b63…2a82056e` | `tamper-evident-match` |
| `run_arm_c_20260524T205020Z_3` | `sha256:1b32b461…51124594` | `tamper-evident-match` |

The digests **differ across runs**: each archive has its own per-run
identity (timestamps, PIDs, inodes, run_id). The binding is therefore
**per-run tamper-evident**, not cross-run byte-identical. That is the
honest claim to publish — a measured-run archive is not bit-stable
across kernel measurements of the "same" workload, but each trace can
still verifiably point at exactly one archive.

### 8. Per-run binding integrity, not cross-run byte determinism

> Arm C validates per-run binding integrity, not cross-run byte
> determinism. Each trace binds to its own archive through
> `assay.archive.manifest_digest`; across the three live eBPF runs,
> archive bytes differ because run IDs, timestamps, process IDs, and
> inode observations vary. The stable claim is shape stability plus
> clean measurement health, not byte-identical archives.

Concretely: the *shape* of the archive is identical across the three
runs (same files present, same per-layer line counts — 2 events,
7 kernel events — same `capability_surface.*` keys, same span tree,
same join key `tc_runner_policy_001`). The *bytes* of `manifest.json`,
`events.ndjson`, and `layers/kernel.ndjson` differ run-to-run, because
they encode timestamps, PIDs, and inodes from the real kernel
measurement. This matches what the v0 archive contract guarantees
today and is why the cross-runtime diff v0 work
(A1+B3+C1 canonicalization, work-dir prefix normalization, side-band
SDK metadata) exists in the first place.

## What v1 still does NOT prove

### Tool-level L1/L2 join is not yet demonstrated (primary v1 limitation)

> Tool-level L1/L2 join is not yet demonstrated in this run because
> the Runner archive has no SDK events. The comparison currently
> joins at run level via `run_id` and `assay.archive.manifest_digest`;
> restoring SDK-layer ingestion is the next prerequisite for
> `gen_ai.tool.call.id` joins and the v1.5 tampering scenario.

In numbers: `observation-health.json` reported `sdk_layer: absent` and
`policy_layer: absent` for all three Arm C runs. `layers/sdk.ndjson`
in the archive is empty (0 lines). The OTel trace correctly carries
`gen_ai.tool.call.id = tc_runner_policy_001`, but the archive side
has no corresponding `sdk_event.tool_call_id` to join against, so
`compare.py` reports `tool_call_id join: archive-side-absent`.

**Root cause:** the workload's in-process OTel `runner.on('agent_tool_start',
...)` hook emits spans into the OTLP trace but does **not** write
events into the `$ASSAY_RUNNER_SDK_EVENT_LOG` NDJSON file that
`assay runner-spike --agent-shim openai-agents` watches and folds
into the archive's SDK layer. The two streams currently live in
parallel without a shared sink.

**v1.5 prerequisite:** wire the workload to write SDK events through
both paths — the OTLP exporter (for L1) and the SDK event log (for
L2) — using the same `tool_call_id`. Once that lands, the comparator
will see joined tool-level evidence, and only then can v1.5's
adversarial tool-call argument tampering scenario be evaluated
meaningfully.

### Other v1 follow-ups

- **Adversarial scenario (v1.5).** Tool-call argument tampering: the
  agent reports `path = /workdir/safe.txt` while the kernel observes
  a normalized traversal to a controlled out-of-workdir target.
  Fixture path stays safe; the claim is the asymmetry, not exfiltration.
  Gated on the SDK-layer fix above.
- **Overhead measurements at statistical power.** Wall-clock at
  `n >= 20`, RSS at `n >= 5`, archive/trace size at `n = 3`. v1 ran
  `n = 3` purely for shape stability; latency claims at `n = 3` are
  not yet defensible.
- **L3 (Tetragon/Falco/Tracee) comparison.** Explicit follow-up in
  the plan doc.

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
