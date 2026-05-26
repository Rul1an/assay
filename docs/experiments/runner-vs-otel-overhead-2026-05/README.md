# Runner vs OTel Overhead Harness

> **Status:** Slice 1 local Arm B harness, Slice 2 delegated Arm C
> dispatch pipeline, Slice 3 RSS collection, and Slice 4 summary/BMF
> rendering. This directory contains the experiment-scoped measurement
> harness and schema sidecars for the plan in
> [`../runner-vs-otel-overhead-2026-05.md`](../runner-vs-otel-overhead-2026-05.md).
> It does not contain committed benchmark results.
>
> Current findings are in [`findings.md`](findings.md). They summarize
> the delegated Arm C host-class baseline and intentionally withhold
> cross-host deltas.

## What This Emits

The harness writes:

- `arm-b-otel/samples.jsonl` using
  `assay.experiment.overhead_sample.v0`;
- `arm-b-otel/summary.json` using
  `assay.experiment.overhead_summary.v0`;
- `arm-b-otel/summary.md`, a reviewer-friendly rendering of the same
  summary;
- `artifacts/bmf.json`, a derived Bencher Metric Format export whose
  metric keys map to `{ "value": ... }` objects;
- `artifacts/trace-sizes.json`, a trace-size side artifact for overhead
  bookkeeping; and
- `artifacts/archive-sizes.json`, an archive-size side artifact that is
  empty for Arm B and populated for Arm C; and
- `artifacts/rss-sizes.json`, a peak-RSS side artifact populated when
  the harness runs with `--measure-rss`.

The experiment schemas are intentionally not Runner archive contracts.
They are local measurement evidence for the overhead follow-up only.

BMF metric keys use the full arm slug so future arms stay
unambiguous: `runner_vs_otel.arm_b_otel.*` for local Arm B and
`runner_vs_otel.arm_c_dual_capture.*` for delegated Arm C.

## Local Smoke

From the repository root:

```bash
python3 docs/experiments/runner-vs-otel-overhead-2026-05/overhead_harness.py \
  --iterations 1 \
  --skip-build \
  --clean \
  --out-dir "$(mktemp -d)/overhead"
```

Add `--measure-rss` to collect peak RSS with `/usr/bin/time`. The
harness currently parses GNU time (`-v`) on Linux and `/usr/bin/time -l`
on macOS.

For the Slice 1 acceptance dry run, use `--iterations 20` and a
temporary output directory. Do not commit generated measurements until
the findings slice decides what should become evidence. The default
`runs/overhead-2026-05/` output path is ignored for this reason.

## Delegated Arm C

Arm C is dispatched manually through
[`runner-otel-overhead-experiment.yml`](../../../.github/workflows/runner-otel-overhead-experiment.yml).
The workflow runs on `assay-bpf-runner`, invokes the same harness with
`--arm arm-c-dual-capture`, uploads `overhead-runs/`, and fails if any
sample is either non-zero exit or capture-unclean.

For the Slice 3 RSS run, dispatch the same workflow with
`repetitions=5` and `measure_rss=true`. Keep `build_ebpf=true` unless a
known-good `target/assay-ebpf.o` is already present on the delegated
runner. The first delegated RSS run passed as
[GitHub Actions run 26454010701](https://github.com/Rul1an/assay/actions/runs/26454010701):
5/5 valid samples, 0 discarded samples, all health gates clean.

The local unit tests exercise the Arm C path with a fake `assay` binary
that emits the expected archive shape. The first validation against real
`assay runner-spike` output happens on the first delegated workflow
dispatch.

Do not commit the uploaded artifacts until the findings slice decides
which measurements should become evidence.

## Tests

```bash
python3 -m unittest discover \
  -s docs/experiments/runner-vs-otel-overhead-2026-05 \
  -p 'test_*.py'
```
