# Runner vs OTel Overhead Harness

> **Status:** Slice 1 local Arm B harness, Slice 2 delegated Arm C
> dispatch pipeline, Slice 3 RSS collection, Slice 4 summary/BMF
> rendering, Slice 5 findings, Slice 6 same-host Arm B dispatches,
> Slice 7 Arm A runner-only dispatches, Slice 8 phase timing diagnostics,
> Slice 9 paired A/C residual diagnostics, Slice 10 smoke validation,
> Slice 11 starter matrix, and Slice 12 boundary-finding have landed. This
> directory contains the
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

The first paired Arm A/C diagnostic passed as
[GitHub Actions run 26479319306](https://github.com/Rul1an/assay/actions/runs/26479319306):
20 valid samples per arm, 0 discarded samples, and matching
`linux-aarch64-6.8.0-117-generic` host class. It showed the Slice 8
Arm A-over-Arm C wall-clock median gap does not reproduce under
adjacent pairing, so wall-clock decomposition stays unpublished.

Slice 10 should not be another broad Arm A/C rerun. The planned next
step is an event-rate / workload-intensity sweep that varies kernel
event count, span/event count, concurrency, and payload size. Its useful
output is a set of slopes and thresholds, such as overhead per 1k kernel
events or the event rate where ring-buffer retrieval becomes visible,
not a product benchmark number.

The delegated workflow exposes the Slice 10 knobs as
`sweep_kernel_event_rate`, `sweep_span_event_rate`, `sweep_concurrency`,
and `sweep_payload_size`. Baseline defaults preserve the pre-Slice-10
behavior. Non-baseline runs embed `assay.experiment.event_rate_sweep.v0`
metadata in each sample and summary so review artifacts remain
self-describing. Slice 12 extended `x500` / `x1000` targets use
`assay.experiment.event_rate_sweep.v0.1` instead of changing the meaning
of v0 labels. Arm A has no OTel trace export, so its sample metadata
records any requested span/event pressure as `baseline` / `0` even when
the paired Arm C sample applies the requested span/event target.

The local unit tests exercise the Arm A / Arm C paths with a fake
`assay` binary that emits the expected archive shape. Real
`assay runner-spike` validation is provided by the delegated workflow
dispatches listed above.

Two post-merge Slice 10 smoke dispatches validated the real workflow
path on main:

- [Run 26508127380](https://github.com/Rul1an/assay/actions/runs/26508127380)
  ran paired A/C with `kernel=low`, `span=baseline`, `concurrency=1`,
  and `payload=small`: 2/2 valid samples per arm, 0 discarded, clean
  health gates.
- [Run 26508355816](https://github.com/Rul1an/assay/actions/runs/26508355816)
  ran paired A/C with `kernel=medium`, `span=low`, `concurrency=2`,
  and `payload=small`: 2/2 valid samples per arm, 0 discarded, clean
  health gates. The artifacts showed `event-rate-sweep/worker-*`
  kernel events for both arms and `assay.sweep.*` trace metadata for
  Arm C.

Those runs are smoke evidence only. They prove the knobs reach the real
workload and fixture paths; they are not sweep findings.

To reproduce the smoke inspection, download the run artifact and inspect
the sample metadata plus captured kernel and trace artifacts:

```bash
gh run download 26508355816 --dir /tmp/assay-slice10-smoke
python3 - <<'PY'
import json
from pathlib import Path

root = Path('/tmp/assay-slice10-smoke/runner-otel-overhead-paired-ac-26508355816')
for arm in ['arm-a-runner-only', 'arm-c-dual-capture']:
    summary = json.loads((root / arm / 'summary.json').read_text())
    print(arm, summary['valid_samples'], summary['discarded_samples'])
    print(summary['event_rate_sweep'])
    mentions = 0
    for kernel in (root / arm).glob('run_*/archive-contents/layers/kernel.ndjson'):
        mentions += kernel.read_text(errors='replace').count('event-rate-sweep/worker-')
    print('kernel sweep mentions', mentions)

trace = (root / 'arm-c-dual-capture' / 'run_001' / 'trace.json').read_text()
print('Arm C sweep attrs', 'assay.sweep.span_events.target' in trace)
print('Arm C span events', trace.count('assay.sweep.span_event'))
PY
```

The smoke runs do not verify rate calibration, payload sizes other than
`small`, concurrency above `2`, or `medium`/`high` span-event behavior.
The first real matrix slice should check observed event counts against
declared targets before interpreting timing.

The predeclared Slice 11 starter matrix is five paired A/C cells with
`repetitions=5`, `measure_rss=false`, and `build_ebpf=true`: control,
kernel-high, span-high, kernel-concurrent, and corner. Its output should
be slopes or thresholds with health gates, not another single broad
wall-clock delta.

That starter matrix has now been dispatched and summarized in
[`findings.md`](findings.md):

- control: [run 26511405031](https://github.com/Rul1an/assay/actions/runs/26511405031)
- kernel-high: [run 26511787316](https://github.com/Rul1an/assay/actions/runs/26511787316)
- span-high: [run 26512146963](https://github.com/Rul1an/assay/actions/runs/26512146963)
- kernel-concurrent: [run 26512515478](https://github.com/Rul1an/assay/actions/runs/26512515478)
- corner: [run 26512909068](https://github.com/Rul1an/assay/actions/runs/26512909068)

All five cells passed with 5/5 valid samples per arm, 0 discarded
samples, clean Runner health gates, and matching host class. The current
finding is a threshold statement: no health boundary was reached at the
starter matrix budget.

The next useful experiment was Slice 12 boundary-finding, not another
single broad A/C wall-clock rerun. The harness supports that slice:
`x500` and `x1000` rate labels emit
`assay.experiment.event_rate_sweep.v0.1`, workflow dispatches can add
warm-up samples with `warmup_iterations`, and delegated jobs have enough
timeout budget for paired widening cells.

The completed Slice 12 dispatches used paired A/C, `repetitions=5`,
`warmup_iterations=1`, `measure_rss=false`, `build_ebpf=true`, and
`timeout_seconds=300`. The output should be a boundary statement, such
as "healthy through X" or "first unhealthy cell at Y", with event-count
calibration and Runner health gates reported before timing is
interpreted. RSS is intentionally not re-measured in this slice.
Warm-up failures do not abort the harness; inspect
`warmup-samples*.jsonl` for their `exit_code`. If every warm-up sample in
a dispatch failed, treat that dispatch as inconclusive even when the
measured samples pass.

That boundary-finding sweep has now been dispatched and summarized in
[`findings.md`](findings.md):

- k500: [run 26517696032](https://github.com/Rul1an/assay/actions/runs/26517696032)
- k1000: [run 26518158603](https://github.com/Rul1an/assay/actions/runs/26518158603)
- s500: [run 26518522754](https://github.com/Rul1an/assay/actions/runs/26518522754)
- s1000: [run 26518894002](https://github.com/Rul1an/assay/actions/runs/26518894002)
- kc1000: [run 26519398461](https://github.com/Rul1an/assay/actions/runs/26519398461)
- corner-lite: [run 26520226593](https://github.com/Rul1an/assay/actions/runs/26520226593)

All six cells passed with 5/5 measured samples per arm, 0 discarded
samples, successful warm-up samples, clean Runner health gates, and
matching host class. Kernel-event calibration matched targets through
1000 worker files per measured sample. The first widened span cell
(`s500`) retained only 128/500 Arm C span events, and later span cells
retained 128/1000. The current default-config boundary is therefore OTel
span-event fidelity, not Runner health.

Do not commit the uploaded artifacts until the findings slice decides
which measurements should become evidence.

## Tests

```bash
python3 -m unittest discover \
  -s docs/experiments/runner-vs-otel-overhead-2026-05 \
  -p 'test_*.py'
```
