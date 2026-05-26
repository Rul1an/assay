# v1 Findings: Runner Archives Next to OTel Traces

> **Status:** Slices 1–3 evidence landed on `main`. Arm B baseline
> (macOS local, n=3 + dual-simulation), Arm C baseline (delegated
> `assay-bpf-runner`, n=3, real Linux/eBPF), Slice 2 (SDK-layer
> ingestion fix, n=3, tool-level L1↔L2 join working), and Slice 3
> (tool-call argument tampering, n=3, `intent_effect_status =
> intent-effect-mismatch` on the agent's reported fictional path) all
> have committed per-run evidence. Arm A (Runner-only) is implicitly
> covered by the archive half of Arm C. **Slice 4 publication drafts**
> (OpenInference discussion + blog) are committed under
> [`publication/`](publication/). The narrow OpenInference vocabulary
> question is filed as
> [Arize-ai/openinference#3162](https://github.com/Arize-ai/openinference/issues/3162);
> the blog remains unpublished pending maintainer triage. Overhead
> measurement at n≥20 and the L3 (Tetragon/Falco/Tracee) comparison
> remain open follow-ups.
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

## Slice 2 resolution: SDK-layer ingestion + tool-level L1↔L2 join

The primary v1 limitation flagged above is now resolved. Three new
Arm C iterations were run on the delegated host with the Slice 2
workload + workflow change applied; the evidence is under
[`runs/slice2-arm-c/`](runs/slice2-arm-c/) (GitHub Actions run
[`26373111099`](https://github.com/Rul1an/assay/actions/runs/26373111099),
head `0a1e6884`).

### Resolution mechanics

Two changes, both small:

- `workload/src/sdk-events.ts` (new): NDJSON emitter that writes
  `assay.runner.sdk_event.v0` events into the path provided via
  `$ASSAY_RUNNER_SDK_EVENT_LOG`. When the env var is not set
  (Arm B local), the emitter is a no-op so existing behaviour is
  preserved exactly.
- `workload/src/workload.ts`: emit `tool_call_started`,
  `tool_call_completed`, and `run_finished` events through the SDK
  emitter alongside the existing OTel tool spans. Both streams share
  the same `tool_call_id`, so the comparator can join L1 to L2 at
  tool level.
- `.github/workflows/runner-otel-experiment.yml`: pass
  `--sdk-event-log $RUN_DIR/sdk-events.ndjson` to
  `assay runner-spike run`, which is what triggers the env-var
  injection into the workload child and the post-run fold of the
  NDJSON into the archive (see `build_spec()` in
  `crates/assay-cli/src/cli/commands/runner_spike.rs` line 85). The
  `--agent-shim openai-agents` flag alone does not trigger this.

### Before vs after

| Metric | Pre-Slice 2 (`runs/v1-arm-c/`) | Post-Slice 2 (`runs/slice2-arm-c/`) |
|---|---|---|
| `archive_sdk_events` | 0 | **3 per run** |
| `tool_call_id_join` | `archive-side-absent` | **`joined:tc_runner_policy_001`** |
| `observation_health.sdk_layer` | `absent` | **`self_reported`** |
| `archive.sdk_tools` | `[]` | **`["read_file"]`** |
| `archive.sdk_tool_call_ids` | `[]` | **`["tc_runner_policy_001"]`** |
| `manifest_digest_binding` | `tamper-evident-match` | `tamper-evident-match` (preserved) |
| Health gates (`kernel_layer`, `ringbuf_drops`, `cgroup_correlation`) | clean | clean (preserved) |

### What this enables

The central claim now upgrades from run-level binding to **run-level
binding + tool-level join**. The comparator can now demonstrate, on
real Linux/eBPF measurement data, that:

- the same `tool_call_id` is present in both the OTel trace
  (`gen_ai.tool.call.id`) and the Runner archive's SDK layer, joined
  at tool granularity; and
- the manifest-digest binding still holds per-run.

Slice 3 (adversarial tool-call argument tampering) can now be built
on top of this evidence: with tool-level join working, the
comparator can express "the trace reported tool with argument X, the
kernel measured effect Y, and X ≠ Y at the same `tool_call_id`."
Without Slice 2 that claim could not be made.

## Slice 3 resolution: tool-call argument tampering demonstrated

The central asymmetry claim of the experiment now has delegated
evidence under [`runs/slice3-arm-c/`](runs/slice3-arm-c/) (GitHub
Actions run
[`26373990933`](https://github.com/Rul1an/assay/actions/runs/26373990933),
head `3dec3e1e`).

### Mechanics

Two opt-in workload flags activate the scenario:

- `--tampering` redirects the `read_file` tool's actual read from the
  reported path to a controlled `${workDir}/tampering-target.txt`.
  The deterministic model is also flipped to *report* a fictional
  path (`${workDir}/agent-claimed-fixture.txt`) that the workload
  never creates, so the kernel cannot record it under any honest
  open.
- `--capture-sensitive-otel-content` (auto-enabled by `--tampering`)
  populates `gen_ai.tool.call.arguments` on the tool span, per the
  OTel GenAI semconv, so the trace carries the reported intent that
  the comparator needs.

Workflow `tampering_mode=true` passes the flags through.

### What the three Arm C runs show

For all three iterations:

| Field | Value |
|---|---|
| `intent_effect_status` | `intent-effect-mismatch:.../agent-claimed-fixture.txt` |
| reported tool argument (L1 trace) | `${workdir}/agent-claimed-fixture.txt` (a path the workload never creates) |
| measured kernel paths (L2 archive) | includes `tampering-target.txt` (redirected read), `openai-agents-input.txt` (workload pre-write), workload JS files; **does not** include `agent-claimed-fixture.txt` |
| `tool_call_id_join` | `joined:tc_runner_policy_001` (Slice 2 preserved) |
| `manifest_digest_binding` | `tamper-evident-match` (Slice 1 preserved) |
| health gates | clean |

The reported intent and the measured effect share a `tool_call_id`,
which is what makes the divergence claimable: both sides agree on
which tool call we are talking about, but disagree on what that tool
call actually did. The OTel trace says the agent opened
`agent-claimed-fixture.txt`; the kernel says the agent opened
`tampering-target.txt`. Reported intent ≠ measured effect.

### Why the demonstration is honest

The tampering target's content is harmless and lives entirely inside
the experiment's scratch workdir. The reported path is deliberately
fictional so the divergence cannot be a false positive caused by the
workload pre-creating the file the agent claims to read (that was
the failure mode of the first Slice 3 dispatch; see commit `3dec3e1e`
for the fix and the workflow run history for the contrast).

### Kernel-event v0 rerun

After the Runner kernel-event line schema was tightened, Slice 3 was
rerun on Arm C under the same tampering scenario. The new evidence lives
under
[`runs/slice3-arm-c-kernel-event-v0/`](runs/slice3-arm-c-kernel-event-v0/)
and preserves the original Slice 3 result:

- `manifest_digest_binding=tamper-evident-match`
- `tool_call_id_join=joined:tc_runner_policy_001`
- `intent_effect_status=intent-effect-mismatch:<workdir>/agent-claimed-fixture.txt`
- `kernel_layer=complete`, `ringbuf_drops=0`,
  `cgroup_correlation=clean`

The difference is that `layers/kernel.ndjson` now carries open metadata
such as `access_mode`, `operation_flags`, `status`, and `return_value`.
The redirected `tampering-target.txt` appears as a successful kernel
read, while workload-created files and logs appear as writes with create
and truncate/append flags. This upgrades the evidence from path presence
alone to operation-aware measured effects, without changing the original
trace/archive binding claim.

### What this enables next

With reported intent and measured effect joined at the same
`tool_call_id` and provably divergent on a controlled scenario, the
experiment can publish a vocabulary discussion on OpenInference / OTel
GenAI semconv asking for the right place to put a runtime-evidence
binding attribute and an intent-vs-effect status. That is Slice 4 of
the experiment plan.

## Slice 4: publication drafts (not yet filed)

Slice 4 produces two drafts, both committed under
[`publication/`](publication/) so the wording matches the evidence
on disk before either goes out:

- [`publication/openinference-discussion.md`](publication/openinference-discussion.md)
  — single narrow vocabulary question for the OpenInference / OTel
  GenAI WG: where should `agent.runtime_evidence.{digest, health,
  boundary, intent_effect_status}` live? Channels: file on
  [`arize-ai/openinference`](https://github.com/Arize-ai/openinference/discussions)
  first; cross-link to OTel semconv only if routed there. No
  individual maintainer pings.
- [`publication/blog-draft.md`](publication/blog-draft.md) — engineer
  audience write-up covering the four slices, the per-run binding,
  the tool-level join, and the intent-vs-effect mismatch on real
  eBPF data. Posts only after the OpenInference discussion has at
  least one maintainer response, so the blog can embed the
  discussion link and the back-and-forth.

The publication discipline (one channel at a time, no
@-mentions, no multi-question dumps, no adoption asks) is
documented in [`publication/README.md`](publication/README.md).
Neither artifact is published yet.

## What still does NOT prove

- **Overhead measurements at statistical power.** Wall-clock at
  `n >= 20`, RSS at `n >= 5`, archive/trace size at `n = 3`.
  v1/Slice 2/Slice 3 each ran `n = 3` purely for shape stability;
  latency claims at `n = 3` are not yet defensible. The measurement
  follow-up is scoped in
  [`../runner-vs-otel-overhead-2026-05.md`](../runner-vs-otel-overhead-2026-05.md).
- **L3 (Tetragon/Falco/Tracee) comparison.** Explicit follow-up in
  the plan doc.
- **Kernel-event granularity beyond capability_surface.** The
  current comparator checks "reported path ∈ measured paths". A
  v2 comparator could distinguish opens-for-read from opens-for-write
  by parsing `layers/kernel.ndjson` directly, which would make the
  intent-vs-effect signal more precise. v0 capability_surface is
  sufficient for the current tampering demonstration.

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

# Compare against the synthetic fixture archive (Arm B has no real
# archive). Paths below are repo-root relative, so cd up to the repo
# root from the workload directory (four levels: workload ->
# runner-vs-otel-2026-05 -> experiments -> docs -> repo root).
cd ../../../..
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
