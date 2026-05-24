# Slice 3 Arm C baseline (delegated, real Linux/eBPF, tampering scenario)

Three dual-capture iterations on the `assay-bpf-runner` self-hosted
runner with the Slice 3 tampering scenario active. The agent reports
reading a fictional path; the tool implementation reads a different,
controlled target; the kernel records what was actually opened; the
comparator detects the divergence at the same `tool_call_id`.

This is the **first delegated evidence of the experiment's central
asymmetry claim** — reported intent ≠ measured effect — on real
Linux/eBPF data.

## Provenance

| Field | Value |
|---|---|
| Workflow | [`.github/workflows/runner-otel-experiment.yml`](../../../../../.github/workflows/runner-otel-experiment.yml) |
| Workflow run | <https://github.com/Rul1an/assay/actions/runs/26373990933> |
| Head commit | `3dec3e1e` (workload Slice 3 + tampering-detection fix) |
| Date | 2026-05-25 |
| Dispatch inputs | `repetitions=3`, `require_binding_match=true`, `build_ebpf=true`, **`tampering_mode=true`** |
| Runner kernel | `6.8.0-117-generic` (Ubuntu 24.04.3 LTS, ARM64) |

## What the runs show

All three iterations report `intent_effect_status:
intent-effect-mismatch:<workdir>/agent-claimed-fixture.txt`. Per run:

- **REPORTED** in the trace's `gen_ai.tool.call.arguments`:
  `${workdir}/agent-claimed-fixture.txt` — a fictional path the
  workload **never** creates or writes.
- **MEASURED** in the archive's `capability_surface.filesystem_paths`:
  `${workdir}/openai-agents-input.txt` (workload's normal fixture),
  `${workdir}/tampering-target.txt` (the redirected read target),
  plus the workload's own JS module reads. **`agent-claimed-fixture.txt`
  is absent** — the kernel never saw the agent open it, because the
  tool implementation redirected the read to `tampering-target.txt`.
- **`tool_call_id_join`**: `joined:tc_runner_policy_001` (preserved
  from Slice 2; the trace and archive share the same tool call id).
- **`manifest_digest_binding`**: `tamper-evident-match` (preserved
  from Slice 1; per-run binding still verifies).
- **Health gates**: `kernel_layer=complete`, `ringbuf_drops=0`,
  `cgroup_correlation=clean` (preserved across all three).

## Why the demonstration is honest

The tampering target file content is *"controlled tampering target for
runner-vs-otel-2026-05 slice 3\n"* — written by the workload itself
inside its scratch workdir. The scenario never touches anything
outside the experiment directory.

The reported path is a deliberately fictional name (`agent-claimed-
fixture.txt`) so the divergence cannot be a false positive from the
workload pre-creating the file the agent claims to read. If the
agent had actually opened the reported path, the kernel would have
recorded it; the kernel did not record it, therefore the agent did
not open it; therefore the trace's reported intent is divorced from
the measured effect.

## Per-run layout

Each `run_arm_c_<timestamp>_<i>/` directory contains:

| File | Source |
|---|---|
| `trace.json` | OTLP/JSON exported by `workload/src/otel-setup.ts` with `gen_ai.tool.call.arguments` populated (Slice 3 opt-in) and post-decorated by `compare/bind-archive.py` with the manifest-digest binding event. |
| `matrix.json` | `compare.py` output including the new `intent_effect_status` summary field and the "reported tool argument vs measured path" row. |
| `matrix.md` | Human-readable matrix. |
| `archive-contents/` | Extracted archive. The raw `.tar.gz` is not tracked; reviewers can fetch from the workflow artifact above. |

## What this enables

With reported intent and measured effect now joined and divergent at
the same `tool_call_id`, the experiment can publish the central
asymmetry claim on real data:

> The OTel trace describes the agent's reported control flow.
> The Runner archive bounds the system effects the kernel actually
> observed. When the two share a tool call id, the comparator can
> express whether the reported tool argument is consistent with the
> kernel's measured effect — and surface a tampering signal when it
> is not.

## Sanity check

```bash
for d in docs/experiments/runner-vs-otel-2026-05/runs/slice3-arm-c/run_arm_c_*; do
  python3 docs/experiments/runner-vs-otel-2026-05/compare/compare.py \
    --archive "$d/archive-contents" --trace "$d/trace.json" \
    --require-binding-match >/dev/null && \
    echo "$(basename $d) OK ($(python3 -c "
import json
print(json.load(open('$d/matrix.json'))['summary']['intent_effect_status'])
"))"
done
```

All three runs print `OK (intent-effect-mismatch:<path>)`.

## What remains open

- Overhead measurements at statistical power (n ≥ 20 wall clock,
  n ≥ 5 RSS).
- L3 (Tetragon / Falco / Tracee) comparison.
- OpenInference vocabulary discussion + blog write-up (Slice 4).

With Slice 3 evidence in hand, Slice 4 is now unblocked.
