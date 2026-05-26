# Runner vs OTel Overhead Harness

> **Status:** Slice 1 local Arm B harness, Slice 2 delegated Arm C
> dispatch pipeline, Slice 3 RSS collection, Slice 4 summary/BMF
> rendering, Slice 5 findings, Slice 6 same-host Arm B dispatches, and
> Slice 7 Arm A runner-only dispatches. Slice 8 phase timing diagnostics
> have landed for Arm A/C, and Slice 9 paired A/C residual diagnostics
> are ready to dispatch. This directory contains the
> experiment-scoped measurement
> harness and schema sidecars for the plan in
> [`../runner-vs-otel-overhead-2026-05.md`](../runner-vs-otel-overhead-2026-05.md).
> It does not contain committed benchmark results.
>
> Current findings are in [`findings.md`](findings.md). They summarize
> the delegated same-host Arm A / Arm B / Arm C measurement set and keep
> the caveats attached.

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
  empty for Arm B and populated for Arm A / Arm C; and
- `artifacts/rss-sizes.json`, a peak-RSS side artifact populated when
  the harness runs with `--measure-rss`; and
- `artifacts/phase-timings.json`, an experiment-scoped side artifact
  populated for Arm A / Arm C when `assay runner-spike` emits phase
  timing diagnostics; and
- `artifacts/paired-sequence.json`, emitted only by `arm=paired-a-c`,
  which carries `assay.experiment.paired_sequence.v0` and records
  adjacent pair order and per-sample phase residuals.

The experiment schemas are intentionally not Runner archive contracts.
They are local measurement evidence for the overhead follow-up only.

BMF metric keys use the full arm slug so future arms stay
unambiguous: `runner_vs_otel.arm_b_otel.*` for local Arm B and
`runner_vs_otel.arm_c_dual_capture.*` for delegated Arm C. Arm A uses
`runner_vs_otel.arm_a_runner_only.*`.

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

## Delegated Arm A / Arm B / Arm C

Arm B and Arm C are dispatched manually through
[`runner-otel-overhead-experiment.yml`](../../../.github/workflows/runner-otel-overhead-experiment.yml).
The workflow runs on `assay-bpf-runner` and exposes an `arm` input:

- `arm-a-runner-only` runs `--arm arm-a-runner-only` with
  `assay runner-spike`, eBPF, and the deterministic OpenAI Agents
  fixture, but without OTel trace export.
- `arm-c-dual-capture` runs `--arm arm-c-dual-capture` with
  `assay runner-spike`, eBPF, and Runner health gates.
- `arm-b-otel` runs `--arm arm-b-otel` on the same delegated host class
  without Runner capture.
- `paired-a-c` runs Arm A and Arm C in one delegated job as adjacent,
  counterbalanced pairs. Use this only for residual diagnostics after
  broad Arm A/C phase timing has already landed.

Arm A and Arm C fail if any sample is either non-zero exit or
capture-unclean. Arm B fails if any sample exits non-zero. A direct arm
delta is only valid when separately dispatched summaries emit matching
`host_class` values.

For the Slice 3 RSS run, dispatch the same workflow with
`arm=arm-c-dual-capture`, `repetitions=5`, and `measure_rss=true`. Keep
`build_ebpf=true` unless a known-good `target/assay-ebpf.o` is already
present on the delegated runner. The first delegated RSS run passed as
[GitHub Actions run 26454010701](https://github.com/Rul1an/assay/actions/runs/26454010701):
5/5 valid samples, 0 discarded samples, all health gates clean.

The first same-host Arm B wall-clock run passed as
[GitHub Actions run 26459699303](https://github.com/Rul1an/assay/actions/runs/26459699303):
20/20 valid samples and 0 discarded samples. The matching Arm B RSS run
passed as
[GitHub Actions run 26461726436](https://github.com/Rul1an/assay/actions/runs/26461726436):
5/5 valid samples and 0 discarded samples. Both emitted the same
`linux-aarch64-6.8.0-117-generic` host class as Arm C.

Arm A is the optional decomposition arm. Dispatch it only when you need
to split the current Arm C delta into "Runner archive only" versus
"Runner archive plus OTel trace": first `arm=arm-a-runner-only`,
`repetitions=20`, `measure_rss=false`, then `repetitions=5`,
`measure_rss=true`.

The first Arm A wall-clock run passed as
[GitHub Actions run 26463798358](https://github.com/Rul1an/assay/actions/runs/26463798358):
20/20 valid samples and 0 discarded samples. The matching Arm A RSS run
passed as
[GitHub Actions run 26464003194](https://github.com/Rul1an/assay/actions/runs/26464003194):
5/5 valid samples and 0 discarded samples. Both emitted the same
`linux-aarch64-6.8.0-117-generic` host class as Arm B and Arm C.

Diagnostic repeat run
[26472122983](https://github.com/Rul1an/assay/actions/runs/26472122983)
failed because the harness discarded one Arm A sample. Failed harness
runs now still upload partial `overhead-runs/` artifacts when available;
those artifacts are diagnostic evidence and should not be promoted to
findings unless the findings text explicitly explains why the sample was
discarded.

Follow-up repeat run
[26473448298](https://github.com/Rul1an/assay/actions/runs/26473448298)
passed with 20/20 valid samples, 0 discarded samples, and the same
`linux-aarch64-6.8.0-117-generic` host class. Its p99/median ratio was
healthy, but Arm A remained slower than Arm C at the median, so
that run motivated phase timing rather than another broad comparison.

Phase-timing runs
[26476490968](https://github.com/Rul1an/assay/actions/runs/26476490968)
(Arm A) and
[26476824593](https://github.com/Rul1an/assay/actions/runs/26476824593)
(Arm C) passed with 20/20 valid samples each. They show that the
instrumented Runner phases explain part, but not all, of the Arm A /
Arm C median wall-clock gap. The largest instrumented phase delta is
`monitor_attach_ms`; the remaining residual keeps additive wall-clock
decomposition withheld.

Phase timing is emitted as experiment-scoped diagnostics in
`phase_timings_ms` on each sample and aggregated into `summary.json`.
It is not a Runner archive contract and must not replace raw
`wall_clock_ms`.

For Slice 9, dispatch the same workflow with `arm=paired-a-c`,
`repetitions=20`, `measure_rss=false`, and `build_ebpf=true`. The
paired mode writes both Arm A and Arm C summaries plus
`artifacts/paired-sequence.json`. That manifest records the actual
`A/C`, then `C/A`, adjacent-pair order and the derived
`phase_residual_ms` value for each sample. Negative residuals are
diagnostic noise signals, not overhead claims.

The local unit tests exercise the Arm A / Arm C paths with a fake `assay` binary
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
