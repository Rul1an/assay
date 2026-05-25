# runs/

Per-arm Runner archives + per-iteration drift reports for the
cross-runtime-drift-2026-05 experiment.

> **Status:** empty. Slice 3 ships the dispatch workflow
> (`.github/workflows/cross-runtime-drift-experiment.yml`); actual
> baselines arrive in a follow-up commit after the maintainer dispatches
> the workflow with the required API secrets.

## Expected layout (post-dispatch)

```
runs/
  a0/                     # Arm A — OpenAI Agents, n>=3 captures
    run_arm_a-openai_<ts>_1/
      archive.tar.gz
      sdk-events.ndjson
      workdir/
        fixture-input.txt
        fixture-output.txt
        tool-calls.ndjson
        run-meta.json
    run_arm_a-openai_<ts>_2/
    run_arm_a-openai_<ts>_3/
  b0/                     # Arm B — Gemini GenAI, n>=3 captures
    run_arm_b-gemini_<ts>_1/
    run_arm_b-gemini_<ts>_2/
    run_arm_b-gemini_<ts>_3/
  drift/                  # drift.py output per (A_i, B_i) pair
    drift_pair_1.json
    drift_pair_1.md
    drift_pair_2.json
    drift_pair_2.md
    drift_pair_3.json
    drift_pair_3.md
```

## Dispatch procedure

The actual experiment runs on the delegated `assay-bpf-runner`
self-hosted runner; only the maintainer can dispatch.

1. Ensure repo secrets `OPENAI_API_KEY` and `GOOGLE_API_KEY` are set
   in **Settings → Secrets and variables → Actions**. The workflow
   fails fast with a clear error message if either is missing.
2. Go to **Actions → Cross-Runtime Drift Experiment → Run workflow**.
3. Pick `repetitions = 3` (or more for shape stability beyond n=3),
   leave `build_ebpf = true`.
4. Wait for all three jobs to complete: `arm-a-openai`,
   `arm-b-gemini`, `drift-compare`.
5. Download the three artifacts produced by the run:
   `cross-runtime-drift-arm-a-openai-<id>`,
   `cross-runtime-drift-arm-b-gemini-<id>`,
   `cross-runtime-drift-reports-<id>`.

## Committing baselines

After downloading the artifacts:

1. Extract `arm-a-openai` artifact into `runs/a0/`.
2. Extract `arm-b-gemini` artifact into `runs/b0/`.
3. Extract `drift-reports` artifact into `runs/drift/`.
4. Verify each archive's measurement health locally:

   ```bash
   python3 docs/experiments/cross-runtime-drift-2026-05/compare/health_gate.py \
     --archive path/to/run_arm_a-openai_<ts>_<i>/archive.tar.gz
   ```

   This is the same gate the workflow runs per iteration. It exits 0
   only when `ringbuf_drops == 0`, `kernel_layer == "complete"`, and
   `cgroup_correlation == "clean"`. (`assay evidence lint` is for Assay
   *evidence bundles* — a different artifact shape — and does not
   apply here.)

   If you want to eyeball the raw values:

   ```bash
   tar -xOzf archive.tar.gz observation-health.json | python3 -m json.tool
   tar -xOzf archive.tar.gz correlation-report.json | python3 -m json.tool
   ```

   Discard any run where the health gate fails and re-dispatch — the
   dropped events break the kernel-layer completeness invariant the
   experiment depends on.
5. Open a follow-up PR titled
   `docs(experiments): Slice 3 — live Arm A0 + B0 baselines + drift reports`.

The findings doc (Slice 4) then reads these committed baselines and
the drift reports.

## What this directory does NOT contain

- No traces. The runner-vs-otel-2026-05 experiment already proved the
  OTel binding pattern; the cross-runtime-drift experiment compares
  Runner archives directly, no OTel layer required.
- No raw kernel-event ndjson outside the archive's `layers/`. Per the
  plan-doc, kernel-event granularity beyond `capability_surface` v0 is
  an explicit v2-comparator follow-up.
