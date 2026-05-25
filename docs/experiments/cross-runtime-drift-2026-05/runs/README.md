# runs/

Per-arm Runner archives + per-iteration drift reports for the
cross-runtime-drift-2026-05 experiment.

> **Status:** live baseline committed from
> [GitHub Actions run 26394765509](https://github.com/Rul1an/assay/actions/runs/26394765509)
> on head `91d6dbf2`, dispatched with `repetitions=3` and
> `build_ebpf=true`.

## Layout

```
runs/
  a0/                     # Arm A — OpenAI Agents, n=3 captures
    run_arm_a-openai_20260525T100626Z_1/
      archive.tar.gz
      sdk-events.ndjson
      workdir/
        fixture-input.txt
        fixture-output.txt
        tool-calls.ndjson
        run-meta.json
    run_arm_a-openai_20260525T100636Z_2/
    run_arm_a-openai_20260525T100645Z_3/
  b0/                     # Arm B — Gemini GenAI, n=3 captures
    run_arm_b-gemini_20260525T100327Z_1/
    run_arm_b-gemini_20260525T100331Z_2/
    run_arm_b-gemini_20260525T100334Z_3/
  drift/                  # drift.py output per (A_i, B_i) pair
    drift_pair_1.json
    drift_pair_1.md
    drift_pair_2.json
    drift_pair_2.md
    drift_pair_3.json
    drift_pair_3.md
```

## Provenance

| Field | Value |
|---|---|
| Workflow | `Cross-Runtime Drift Experiment` |
| Run | <https://github.com/Rul1an/assay/actions/runs/26394765509> |
| Head SHA | `91d6dbf2` |
| Runner | `assay-bpf-runner` |
| Capture | `assay runner-spike`, Linux/eBPF + cgroup v2 |
| Arm A | `openai-agents`, model `gpt-4o-mini` |
| Arm B | `gemini-genai`, model `gemini-2.5-flash` |

All six archives passed the workflow health gate before artifact upload:
`ringbuf_drops == 0`, `kernel_layer == "complete"`, and
`cgroup_correlation == "clean"`. The workflow also ran the workload
contract-checker per iteration before upload.

## Local verification

Verify committed archive health:

```bash
export REPO_ROOT="$(git rev-parse --show-toplevel)"

for archive in \
  "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/a0/*/archive.tar.gz \
  "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/b0/*/archive.tar.gz
do
  python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/health_gate.py" \
    --archive "$archive"
done
```

Inspect raw health values:

```bash
tar -xOzf runs/a0/run_arm_a-openai_20260525T100626Z_1/archive.tar.gz \
  observation-health.json | python3 -m json.tool
tar -xOzf runs/b0/run_arm_b-gemini_20260525T100327Z_1/archive.tar.gz \
  observation-health.json | python3 -m json.tool
```

The `workdir/tool-calls.ndjson` files preserve the absolute paths used
on the delegated runner (`/opt/actions-runner/_work/...`). That is
intentional: these are committed evidence artifacts, not relocated local
workdirs. The contract-checker already ran in the workflow before upload.
For local inspection, read `tool-calls.ndjson` and `run-meta.json`
directly rather than re-running the checker with relocated paths.

## What this directory does NOT contain

- No traces. The runner-vs-otel-2026-05 experiment already proved the
  OTel binding pattern; the cross-runtime-drift experiment compares
  Runner archives directly.
- No raw kernel-event ndjson outside the archive's `layers/`. Per the
  plan-doc, kernel-event granularity beyond `capability_surface` v0 is
  an explicit v2-comparator follow-up.
