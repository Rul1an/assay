# Slice 2 Arm C baseline (delegated, real Linux/eBPF, SDK-layer ingestion)

Three dual-capture iterations on the `assay-bpf-runner` self-hosted
runner with the Slice 2 SDK-layer ingestion fix applied. This is the
first Arm C evidence in which the archive's `sdk_layer` is populated
and the comparator's `gen_ai.tool.call.id ↔ Runner tool_call_id`
join resolves to `joined:tc_runner_policy_001`.

## Provenance

| Field | Value |
|---|---|
| Workflow | [`.github/workflows/runner-otel-experiment.yml`](../../../../../.github/workflows/runner-otel-experiment.yml) |
| Workflow run | <https://github.com/Rul1an/assay/actions/runs/26373111099> |
| Head commit | `0a1e6884` (workflow `--sdk-event-log` flag + workload SDK emitter) |
| Date | 2026-05-24 |
| Dispatch inputs | `repetitions=3`, `require_binding_match=true`, `build_ebpf=true` |
| Runner kernel | `6.8.0-117-generic` (Ubuntu 24.04.3 LTS, ARM64) |
| Comparator | `compare.py --require-binding-match` (exit 0 for all three) |

## What this baseline proves on top of `runs/v1-arm-c/`

| Metric | `runs/v1-arm-c/` (before Slice 2) | `runs/slice2-arm-c/` (this) |
|---|---|---|
| `archive_sdk_events` | 0 | **3 per run** |
| `tool_call_id_join` | `archive-side-absent` | **`joined:tc_runner_policy_001`** |
| `observation_health.sdk_layer` | `absent` | **`self_reported`** |
| `archive.sdk_tools` | `[]` | **`["read_file"]`** |
| `archive.sdk_tool_call_ids` | `[]` | **`["tc_runner_policy_001"]`** |
| Manifest-digest binding | `tamper-evident-match` | `tamper-evident-match` (preserved) |
| Health gates (`kernel_layer`, `ringbuf_drops`, `cgroup_correlation`) | clean | clean (preserved) |

The previous v1 limitation — "tool-level L1/L2 join is not yet
demonstrated" — is resolved by this baseline.

## Per-run layout

Each `run_arm_c_<timestamp>_<i>/` directory contains:

| File | Source |
|---|---|
| `trace.json` | OTLP/JSON exported by `workload/src/otel-setup.ts` and post-decorated by `compare/bind-archive.py` with the `assay.archive.created` event. |
| `matrix.json` | `compare.py` output. |
| `matrix.md` | Human-readable matrix. |
| `archive-contents/` | Extracted archive (manifest, capability surface, observation health, correlation report, layers/{kernel,policy,sdk}.ndjson, events.ndjson). The raw `.tar.gz` is not tracked; reviewers can re-tar locally or fetch from the workflow artifact above. |

## What still does NOT prove (Slice 3 follow-up)

- Adversarial scenario: tool-call argument tampering. The agent
  reports `path = /workdir/safe.txt`, the kernel observes a normalized
  traversal to a controlled out-of-workdir target. The comparator
  must show **reported intent ≠ measured effect**. With tool-level
  join now working, this is the next observable demonstration.
- Overhead measurements at statistical power (n ≥ 20 wall clock,
  n ≥ 5 RSS).
- L3 (Tetragon/Falco/Tracee) comparison.

## Sanity check

```bash
for d in docs/experiments/runner-vs-otel-2026-05/runs/slice2-arm-c/run_arm_c_*; do
  python3 docs/experiments/runner-vs-otel-2026-05/compare/compare.py \
    --archive "$d/archive-contents" --trace "$d/trace.json" \
    --require-binding-match > /dev/null && \
    echo "$(basename $d) OK"
done
```

All three runs print OK. The matrix's `tool_call_id_join` field
reads `joined:tc_runner_policy_001` for every run, not
`archive-side-absent`.
