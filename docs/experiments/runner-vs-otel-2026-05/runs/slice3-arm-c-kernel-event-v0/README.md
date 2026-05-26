# Slice 3 Arm C rerun with kernel-event v0 open metadata

Three dual-capture iterations on the `assay-bpf-runner` self-hosted
runner with the Slice 3 tampering scenario active, re-run after Runner
kernel events gained explicit open metadata (`access_mode`,
`operation_flags`, `status`, and `return_value`).

This directory does **not** replace
[`../slice3-arm-c/`](../slice3-arm-c/). The original Slice 3 baseline
remains the first delegated proof of the reported-intent vs measured-
effect mismatch. This rerun preserves that result and adds richer
kernel-event evidence.

## Provenance

| Field | Value |
|---|---|
| Workflow | [`.github/workflows/runner-otel-experiment.yml`](../../../../../.github/workflows/runner-otel-experiment.yml) |
| Workflow run | <https://github.com/Rul1an/assay/actions/runs/26442807783> |
| Branch | `codex/runtime-drift-v02-rerender` |
| Comparator/support commit | `08b1664b` |
| Date | 2026-05-26 |
| Dispatch inputs | `repetitions=3`, `require_binding_match=true`, `build_ebpf=true`, **`tampering_mode=true`** |
| Runner | `assay-bpf-runner` |

## What stayed stable

All three iterations preserve the core Slice 3 result:

- `manifest_digest_binding`: `tamper-evident-match`
- `tool_call_id_join`: `joined:tc_runner_policy_001`
- `intent_effect_status`:
  `intent-effect-mismatch:<workdir>/agent-claimed-fixture.txt`
- `kernel_layer`: `complete`
- `ringbuf_drops`: `0`
- `cgroup_correlation`: `clean`
- `sdk_layer`: `self_reported`

## What is new

`archive-contents/layers/kernel.ndjson` now carries operation-aware open
metadata. For example, the tampering target appears as a successful
read:

```json
{
  "schema": "assay.runner.kernel_event.v0",
  "kind": "openat",
  "access_mode": "read",
  "status": "success",
  "return_value": 21,
  "value": ".../workdir/tampering-target.txt"
}
```

That read has no `operation_flags` because it is a plain read. The
workload-created files and logs also show up as writes with create and
truncate/append flags. This means the experiment can now say more than
"the kernel observed this path": it can distinguish the reported tool
argument from a measured successful read of the redirected target, while
still keeping the original path-level matrix for compatibility.

The first `package.json` probe is an expected failed read
(`status=error`, `return_value=-2`) from Node module resolution. It is
captured as evidence, not filtered out.

## Per-run layout

Each `run_arm_c_<timestamp>_<i>/` directory contains:

| File | Source |
|---|---|
| `trace.json` | OTLP/JSON exported by the workload and post-decorated with the archive manifest digest. |
| `matrix.json` | Existing `compare.py` output; path-level claims remain compatible with the original Slice 3 matrices. |
| `matrix.md` | Human-readable matrix. |
| `archive-contents/` | Extracted Runner archive. The raw `.tar.gz` is not tracked; fetch it from the workflow artifact if needed. |

## Sanity check

```bash
for d in docs/experiments/runner-vs-otel-2026-05/runs/slice3-arm-c-kernel-event-v0/run_arm_c_*; do
  python3 docs/experiments/runner-vs-otel-2026-05/compare/compare.py \
    --archive "$d/archive-contents" --trace "$d/trace.json" \
    --require-binding-match >/dev/null && \
    echo "$(basename "$d") OK"
done
```

All three runs should print `OK`.
