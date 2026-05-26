# runs/

Per-arm Runner archives + per-iteration drift reports for the
cross-runtime-drift-2026-05 experiment.

> **Status:** live baseline committed from
> [GitHub Actions run 26398427430](https://github.com/Rul1an/assay/actions/runs/26398427430)
> on head `e3f6ef9d`, dispatched with `repetitions=3` and
> `build_ebpf=true`.

## Layout

```
runs/
  a0/                     # Arm A — OpenAI Agents, n=3 captures
    run_arm_a-openai_20260525T113813Z_1/
      archive.tar.gz
      sdk-events.ndjson
      workdir/
        fixture-input.txt
        fixture-output.txt
        tool-calls.ndjson
        run-meta.json
    run_arm_a-openai_20260525T113821Z_2/
    run_arm_a-openai_20260525T113828Z_3/
  b0/                     # Arm B — Gemini GenAI, n=3 captures
    run_arm_b-gemini_20260525T114112Z_1/
    run_arm_b-gemini_20260525T114117Z_2/
    run_arm_b-gemini_20260525T114122Z_3/
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
| Run | <https://github.com/Rul1an/assay/actions/runs/26398427430> |
| Head SHA | `e3f6ef9d` |
| Runner | `assay-bpf-runner` |
| Capture | `assay runner-spike`, Linux/eBPF + cgroup v2 |
| Arm A | `openai-agents`, model `gpt-4o-mini` |
| Arm B | `gemini-genai`, model `gemini-2.5-flash` |

The A0/B0 archives are the original workflow artifacts. The
`runs/drift/` reports were re-rendered from those committed archives
after the runtime-drift projection schema was frozen as
`assay.runner.runtime_drift.v0.2`; the raw captures were not regenerated.
The comparator at re-render time uses projection, taxonomy, and
provenance fields that did not exist at original capture time. This
asymmetry is intentional: raw evidence is unchanged, while the
projection/report layer is newer and records its own render metadata.
For these committed reports, `provenance.assay_commit` is the original
capture commit (`e3f6ef9d`) and
`provenance.render_metadata.comparator_commit` is the comparator support
commit used for the latest re-render (`2b8ab383`). That re-render keeps
declared projection mappings explicit while summarizing unmatched raw
values by count and sample.
The comparator at and after that support commit emits v0.2 only;
historical v0 reports are retained for reference, not regenerated.

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
tar -xOzf runs/a0/run_arm_a-openai_20260525T113813Z_1/archive.tar.gz \
  observation-health.json | python3 -m json.tool
tar -xOzf runs/b0/run_arm_b-gemini_20260525T114112Z_1/archive.tar.gz \
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
- No raw kernel-event ndjson outside the archive's `layers/`. The drift
  reports now parse optional open metadata from
  `layers/kernel.ndjson` for operation-aware rows; the archive remains
  the source of truth.
