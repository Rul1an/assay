# Runner vs OTel Overhead Harness

> **Status:** Slice 1 local Arm B harness. This directory contains the
> experiment-scoped measurement harness and schema sidecars for the
> plan in [`../runner-vs-otel-overhead-2026-05.md`](../runner-vs-otel-overhead-2026-05.md).
> It does not contain committed benchmark results.

## What This Emits

The local harness writes:

- `arm-b-otel/samples.jsonl` using
  `assay.experiment.overhead_sample.v0`;
- `arm-b-otel/summary.json` using
  `assay.experiment.overhead_summary.v0`;
- `artifacts/bmf.json`, a derived Bencher Metric Format export whose
  metric keys map to `{ "value": ... }` objects.

The experiment schemas are intentionally not Runner archive contracts.
They are local measurement evidence for the overhead follow-up only.

## Local Smoke

From the repository root:

```bash
python3 docs/experiments/runner-vs-otel-overhead-2026-05/overhead_harness.py \
  --iterations 1 \
  --skip-build \
  --clean \
  --out-dir "$(mktemp -d)/overhead"
```

For the Slice 1 acceptance dry run, use `--iterations 20` and a
temporary output directory. Do not commit generated measurements until
the findings slice decides what should become evidence. The default
`runs/overhead-2026-05/` output path is ignored for this reason.

## Tests

```bash
python3 -m unittest discover \
  -s docs/experiments/runner-vs-otel-overhead-2026-05 \
  -p 'test_*.py'
```
