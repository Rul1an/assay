# Runner vs OTel Overhead Findings (2026-05)

> **Status:** same-host findings update. This document summarizes the
> overhead follow-up evidence collected so far. It does not commit the
> generated measurement artifacts. Direct arm deltas are reported only
> for the matching `linux-aarch64-6.8.0-117-generic` host class.

## Evidence Anchors

| Slice | Workflow run | Arm | Samples | Result |
|---|---|---|---:|---|
| 2 | [26449999294](https://github.com/Rul1an/assay/actions/runs/26449999294) | Arm C dual capture | 20 wall-clock | 20 valid, 0 discarded, all health gates clean |
| 3 | [26454010701](https://github.com/Rul1an/assay/actions/runs/26454010701) | Arm C dual capture | 5 RSS | 5 valid, 0 discarded, all health gates clean |
| 6 wall | [26459699303](https://github.com/Rul1an/assay/actions/runs/26459699303) | Arm B OTel-only | 20 wall-clock | 20 valid, 0 discarded, same host class |
| 6 RSS | [26461726436](https://github.com/Rul1an/assay/actions/runs/26461726436) | Arm B OTel-only | 5 RSS | 5 valid, 0 discarded, same host class |
| 7 sanity | [26463582658](https://github.com/Rul1an/assay/actions/runs/26463582658) | Arm A runner-only | 2 wall-clock | 2 valid, 0 discarded, kernel layer complete |
| 7 wall | [26463798358](https://github.com/Rul1an/assay/actions/runs/26463798358) | Arm A runner-only | 20 wall-clock | 20 valid, 0 discarded, same host class |
| 7 RSS | [26464003194](https://github.com/Rul1an/assay/actions/runs/26464003194) | Arm A runner-only | 5 RSS | 5 valid, 0 discarded, same host class |
| 8 diagnostic | [26472122983](https://github.com/Rul1an/assay/actions/runs/26472122983) | Arm A runner-only | 20 wall-clock repeat | failed; one sample discarded, partial artifacts not uploaded by the old workflow |
| 8 repeat | [26473448298](https://github.com/Rul1an/assay/actions/runs/26473448298) | Arm A runner-only | 20 wall-clock repeat | 20 valid, 0 discarded, artifact-success gate active |
| 8 phase A | [26476490968](https://github.com/Rul1an/assay/actions/runs/26476490968) | Arm A runner-only | 20 wall-clock + phase timing | 20 valid, 0 discarded, same host class |
| 8 phase C | [26476824593](https://github.com/Rul1an/assay/actions/runs/26476824593) | Arm C dual capture | 20 wall-clock + phase timing | 20 valid, 0 discarded, same host class |
| 9 paired A/C | [26479319306](https://github.com/Rul1an/assay/actions/runs/26479319306) | Arm A + Arm C paired | 20 adjacent pairs | 20 valid per arm, 0 discarded, same job and host class |
| 10 smoke kernel | [26508127380](https://github.com/Rul1an/assay/actions/runs/26508127380) | Arm A + Arm C paired | 2 adjacent pairs, `kernel=low`, `span=baseline`, `concurrency=1` | 2 valid per arm, 0 discarded, clean health gates |
| 10 smoke span/concurrency | [26508355816](https://github.com/Rul1an/assay/actions/runs/26508355816) | Arm A + Arm C paired | 2 adjacent pairs, `kernel=medium`, `span=low`, `concurrency=2` | 2 valid per arm, 0 discarded, clean health gates |

Generated artifacts from those runs were inspected as review artifacts
only. They are intentionally not committed as benchmark evidence in this
slice.

The failed diagnostic repeat is listed to explain the workflow-DX fix,
not to replace the successful Slice 7 findings. The follow-up repeat
passed with artifacts uploaded and the new artifact-success gate active.
Together, the repeats show that the first-sample cgroup failure is not
deterministic, while wall-clock decomposition still needs phase timing
before it can support an additive claim.

The phase-timing runs are listed as diagnostics, not as replacement
baselines. They validate the Slice 8 instrumentation and localize part
of the Arm A / Arm C median gap, but Arm A again showed an unhealthy
wall-clock tail in that dispatch.

The paired Slice 9 run is also diagnostic. It keeps Arm A and Arm C
adjacent in one delegated job to reduce inter-dispatch drift. It does
not replace the same-host baselines below, and its unhealthy tails keep
wall-clock publication caveats in force.

The Slice 10 smoke runs are workflow and metadata validation only. They
show that the event-rate sweep knobs reach the real delegated workload:
both arms captured `event-rate-sweep/worker-*` kernel events, Arm C
emitted `assay.sweep.*` trace metadata when span/event pressure was
requested, and Arm A correctly recorded `span_event_rate=baseline` with
`target_span_events=0`. They are too small to support slope, threshold,
or benchmark findings.

## Same-Host Baselines

All three arms now have clean measurements on the same delegated
`assay-bpf-runner` host class. The runs were dispatched separately, so
they characterize a shared host class, not co-temporal variance.

| Metric | Value | Interpretation |
|---|---:|---|
| Host class | `linux-aarch64-6.8.0-117-generic` | Delegated Linux runner machine/OS/kernel boundary |
| Arm A wall-clock valid samples | `20/20` | Meets the n >= 20 gate |
| Arm A wall median | `1,859.521 ms` | Runner archive-only repeat baseline on this host class |
| Arm A wall p95 | `2,143.676 ms` | Tail sample remained within the healthy band |
| Arm A wall p99 | `2,459.097 ms` | Nearest-rank p99 for n=20 |
| Arm A wall p99/median | `1.322` | Healthy per the v0 `< 1.5` tail-ratio band |
| Arm A RSS valid samples | `5/5` | Meets the n >= 5 gate |
| Arm A peak RSS median | `116,641,792 bytes` | Runner archive-only memory baseline |
| Arm A peak RSS max | `116,645,888 bytes` | No large RSS outlier in the n=5 sample |
| Arm B wall-clock valid samples | `20/20` | Meets the n >= 20 gate |
| Arm B wall median | `879.961 ms` | OTel-only baseline on this host class |
| Arm B wall p95 | `924.845 ms` | Tail sample remained close to median |
| Arm B wall p99 | `964.023 ms` | Nearest-rank p99 for n=20 |
| Arm B wall p99/median | `1.096` | Healthy per the v0 `< 1.5` tail-ratio band |
| Arm B RSS valid samples | `5/5` | Meets the n >= 5 gate |
| Arm B peak RSS median | `108,953,600 bytes` | OTel-only memory baseline |
| Arm B peak RSS max | `110,493,696 bytes` | No large RSS outlier in the n=5 sample |
| Arm C wall-clock valid samples | `20/20` | Meets the n >= 20 gate |
| Arm C wall median | `1,737.838 ms` | Dual-capture baseline on this host class |
| Arm C wall p95 | `2,051.039 ms` | Tail sample remained close to median |
| Arm C wall p99 | `2,070.354 ms` | Nearest-rank p99 for n=20 |
| Arm C wall p99/median | `1.191` | Healthy per the v0 `< 1.5` tail-ratio band |
| Arm C RSS valid samples | `5/5` | Meets the n >= 5 gate |
| Arm C peak RSS median | `116,649,984 bytes` | Dual-capture memory baseline |
| Arm C peak RSS max | `116,781,056 bytes` | No large RSS outlier in the n=5 sample |
| Arm B trace JSON median | `3,204 bytes` | L1 trace footprint baseline |
| Arm A trace JSON median | `null` | Expected: runner-only arm emits no OTel trace JSON |
| Arm A archive `.tar.gz` median | `1,628 bytes` | L2 compressed archive footprint baseline without trace export |
| Arm A archive extracted median | `5,639 bytes` | Review/storage footprint baseline without trace export |
| Arm C trace JSON median | `3,220 bytes` | L1 trace plus Runner wrapper footprint |
| Arm C archive `.tar.gz` median | `1,776 bytes` | L2 compressed archive footprint baseline |
| Arm C archive extracted median | `8,186 bytes` | Review/storage footprint baseline |

The one-byte artifact-size spread between Slice 2 and Slice 3 is
expected for freshly generated archives and traces. The useful claim is
that the footprint is tiny and stable at this workload scale, not that
archive bytes are deterministic across runs.

## Same-Host Delta

Because Arm B and Arm C emitted the same `host_class`, a narrow
same-host delta is now valid for this deterministic workload. The runs
were not co-temporal, so this is still a host-class baseline comparison,
not a product benchmark.

| Metric | Arm B OTel-only | Arm C dual capture | Delta |
|---|---:|---:|---:|
| Wall median | `879.961 ms` | `1,737.838 ms` | `+857.878 ms` (`+97.5%`) |
| Wall p95 | `924.845 ms` | `2,051.039 ms` | `+1,126.195 ms` (`+121.8%`) |
| Wall p99 | `964.023 ms` | `2,070.354 ms` | `+1,106.331 ms` (`+114.8%`) |
| Wall p99/median | `1.096` | `1.191` | `+0.096` |
| Peak RSS median | `108,953,600 bytes` | `116,649,984 bytes` | `+7,696,384 bytes` (`+7.1%`) |
| Peak RSS max | `110,493,696 bytes` | `116,781,056 bytes` | `+6,287,360 bytes` (`+5.7%`) |
| Trace JSON median | `3,204 bytes` | `3,220 bytes` | `+16 bytes` |

The wall-clock delta is the cost of the current dual-capture path on
this host class: Runner archive capture plus the existing OTel trace
around the deterministic workload. The Arm A section below records the
runner-only decomposition attempt, but that decomposition is not stable
enough to turn this delta into an additive wall-clock cost model.

## Runner-Only Decomposition

Arm A adds the runner-only comparison point: `assay runner-spike` with
Linux/eBPF archive capture and the deterministic OpenAI Agents fixture,
but without OTel trace export. All three arms emitted the same
`host_class`, but the runs were separate dispatches and Arm A uses the
fixture-agent path rather than the OTel workload wrapper. Treat this as
an experiment-scoped decomposition aid, not as a general additive cost
model.

| Metric | Arm B OTel-only | Arm A runner-only | Arm C dual capture |
|---|---:|---:|---:|
| Wall median | `879.961 ms` | `1,859.521 ms` | `1,737.838 ms` |
| Wall p95 | `924.845 ms` | `2,143.676 ms` | `2,051.039 ms` |
| Wall p99 | `964.023 ms` | `2,459.097 ms` | `2,070.354 ms` |
| Wall p99/median | `1.096` | `1.322` | `1.191` |
| Peak RSS median | `108,953,600 bytes` | `116,641,792 bytes` | `116,649,984 bytes` |
| Peak RSS max | `110,493,696 bytes` | `116,645,888 bytes` | `116,781,056 bytes` |
| Trace JSON median | `3,204 bytes` | `null` | `3,220 bytes` |
| Archive `.tar.gz` median | `null` | `1,628 bytes` | `1,776 bytes` |
| Archive extracted median | `null` | `5,639 bytes` | `8,186 bytes` |

The decomposition read is:

- **RSS:** useful and stable. Arm A and Arm C differ by only `8,192`
  bytes at the median RSS level (`0.007%`). The observed same-host RSS
  increase over Arm B is therefore dominated by Runner capture rather
  than by adding the OTel trace wrapper around Runner capture.
- **Wall-clock:** inconclusive as an additive decomposition. The healthy
  Arm A repeat is still `121.683 ms` slower at the median than Arm C,
  even though Arm A omits OTel trace export. That is not a meaningful
  "OTel adds negative overhead" result; it means the runner-only fixture
  path and dual-capture workload path need phase timing before the
  current data can be decomposed into additive cost buckets.

## Phase-Timing Read

Slice 8 added experiment-scoped phase diagnostics via
`assay.experiment.runner_phase_timing.v0`. Both Arm A and Arm C phase
runs used the same `linux-aarch64-6.8.0-117-generic` host class and
produced 20 valid samples with 0 discarded samples.

| Metric | Arm A runner-only | Arm C dual capture | Delta A-C |
|---|---:|---:|---:|
| Wall median | `1,894.545 ms` | `1,787.294 ms` | `+107.251 ms` |
| Wall p95 | `2,542.600 ms` | `2,013.966 ms` | `+528.634 ms` |
| Wall p99 | `6,855.941 ms` | `2,060.190 ms` | `+4,795.751 ms` |
| Wall p99/median | `3.619` | `1.153` | Arm A tail unhealthy |
| Sum of phase medians | `1,427.638 ms` | `1,393.098 ms` | `+34.540 ms` |
| Wall median minus summed phase medians | `466.907 ms` | `394.197 ms` | `+72.711 ms` |

Median phase breakdown:

| Phase | Arm A median | Arm C median | Delta A-C |
|---|---:|---:|---:|
| `preflight_ms` | `0.196 ms` | `0.144 ms` | `+0.052 ms` |
| `cgroup_prepare_ms` | `0.966 ms` | `1.085 ms` | `-0.119 ms` |
| `monitor_attach_ms` | `446.885 ms` | `408.601 ms` | `+38.284 ms` |
| `child_spawn_ms` | `18.020 ms` | `23.787 ms` | `-5.767 ms` |
| `child_runtime_ms` | `850.928 ms` | `847.777 ms` | `+3.151 ms` |
| `event_flush_ms` | `107.313 ms` | `109.086 ms` | `-1.773 ms` |
| `archive_write_ms` | `3.330 ms` | `2.617 ms` | `+0.713 ms` |

The summed phase medians explain about `34.540 ms` of the `107.251 ms`
Arm A median wall-clock gap. The largest instrumented contributor is
`monitor_attach_ms` (`+38.284 ms` for Arm A), but most of the median
gap remains outside the current phase buckets as measured by wall median
minus summed phase medians (`+72.711 ms` residual).

That means Slice 8 supports a narrower conclusion than an additive
wall-clock decomposition: the Runner-internal phases do **not** fully
explain why the runner-only Arm A path is slower than Arm C at the
median. The wall-clock split remains unsuitable for a "Runner archive
only + OTel trace export" additive claim.

## Paired Residual Read

Slice 9 dispatched `arm=paired-a-c` in
[run 26479319306](https://github.com/Rul1an/assay/actions/runs/26479319306).
The harness ran 20 adjacent counterbalanced pairs in one delegated job:
odd pairs used Arm A then Arm C, even pairs used Arm C then Arm A. Both
arms produced 20 valid samples, 0 discarded samples, `ringbuf_drops=0`,
`kernel_layer=complete`, and `cgroup_correlation=clean`.

| Metric | Arm A runner-only | Arm C dual capture | Delta A-C |
|---|---:|---:|---:|
| Wall median | `1,806.007 ms` | `1,917.081 ms` | `-111.074 ms` |
| Wall p95 | `3,500.225 ms` | `4,337.908 ms` | `-837.682 ms` |
| Wall p99 | `3,911.765 ms` | `4,400.113 ms` | `-488.348 ms` |
| Wall p99/median | `2.166` | `2.295` | both tails unhealthy |
| Sum of phase medians | `1,443.657 ms` | `1,499.847 ms` | `-56.190 ms` |
| Wall median minus summed phase medians | `362.349 ms` | `417.234 ms` | `-54.884 ms` |
| Median per-sample `phase_residual_ms` | `368.808 ms` | `391.284 ms` | `-22.476 ms` |
| Median paired wall delta | `n/a` | `n/a` | `-176.852 ms`; noisy pair spread |
| Median paired residual delta | `n/a` | `n/a` | `-26.187 ms`; residuals close |

Median phase breakdown from the paired run:

| Phase | Arm A median | Arm C median | Delta A-C |
|---|---:|---:|---:|
| `preflight_ms` | `0.171 ms` | `0.157 ms` | `+0.014 ms` |
| `cgroup_prepare_ms` | `1.941 ms` | `2.080 ms` | `-0.140 ms` |
| `monitor_attach_ms` | `423.719 ms` | `428.631 ms` | `-4.912 ms` |
| `child_spawn_ms` | `15.442 ms` | `16.860 ms` | `-1.418 ms` |
| `child_runtime_ms` | `887.384 ms` | `939.790 ms` | `-52.406 ms` |
| `event_flush_ms` | `112.041 ms` | `108.897 ms` | `+3.144 ms` |
| `archive_write_ms` | `2.960 ms` | `3.432 ms` | `-0.471 ms` |

This paired result changes the wall-clock read: the Slice 8 Arm A
slower-than-Arm-C median gap does **not** reproduce when the arms run as
adjacent counterbalanced pairs. In the paired run, Arm A is faster at
the median and the per-sample residual medians differ by only
`22.476 ms`. The result points to inter-dispatch drift and measurement
variance as material contributors to the earlier wall-clock anomaly.

The paired run does **not** justify a new additive wall-clock model:
both paired tails are unhealthy (`p99/median > 2.0`), and the paired
wall deltas have a wide spread. It does justify a stopping rule for this
arc: wall-clock decomposition is not stable enough at n=20 on this
runner to publish as an additive split. RSS remains the clean
decomposition signal.

## What This Means

- The delegated measurement harness is usable for all three arms: wall-clock
  and RSS runs produced all-valid samples on the same host class.
- The observed Arm C tail ratio is healthy for this deterministic
  workload on `assay-bpf-runner`.
- The observed Arm C median wall-clock is about 2x Arm B on the same
  host class for this workload, while RSS increases by about 7%.
- The observed Arm A and Arm C RSS medians are effectively identical at
  this scale, so the RSS delta versus Arm B is attributable to Runner
  capture rather than trace JSON export.
- Arm A's repeat wall-clock tail was healthy, but the later phase-timing
  run had an unhealthy tail and the runner-only median remained higher
  than Arm C, so wall-clock decomposition remains a caution, not a
  benchmark claim.
- Slice 8 phase timing localizes the largest measured internal phase
  delta to monitor attach, but the majority of the Arm A / Arm C median
  gap sits outside the current phase buckets.
- Slice 9 paired diagnostics show that the Slice 8 Arm A-over-Arm C
  median gap does not reproduce under adjacent pairing. Wall-clock
  residuals are close enough, and tails noisy enough, that the wall-clock
  decomposition should stop rather than spawning another broad rerun.
- The RSS path works on the delegated Linux runner with GNU
  `/usr/bin/time -v`; samples record the RSS tool version and emit
  `peak_rss_bytes` into both `summary.json` and the BMF export.
- The summary renderer now gives reviewers the same metrics in
  `summary.md` and in the GitHub step summary, while `summary.json`
  remains canonical.

## What This Does Not Mean

- No product ranking is implied between OpenTelemetry, OpenInference, or
  Assay-Runner.
- No model/provider latency claim is made. The workload is deterministic
  and measurement-scoped.
- No co-temporal variance claim is made. Arm B and Arm C ran on the same
  host class at different times, and Arm A was dispatched separately as
  well.
- No additive wall-clock decomposition claim is made between "Runner
  archive only" and "Runner archive plus OTel trace". The phase-timing
  and paired residual runs show that the median gap is not stable under
  pairing, and the paired run has unhealthy tails.
- No Trust Card or Trust Basis claim is added. This remains an
  experiment-scoped measurement follow-up.
- The generated artifacts remain review artifacts until a later decision
  explicitly promotes a measurement bundle into committed evidence.

## Next Work

The correct publication language is now:

> On the `linux-aarch64-6.8.0-117-generic` delegated runner host class,
> the current dual-capture path measured roughly +858 ms median
> wall-clock and +7.7 MB median RSS over OTel-only for this deterministic
> workload. The result is not co-temporal and does not decompose Runner
> archive-only cost.

Arm A measurements and phase-timing diagnostics have landed. The safe
publication language is now:

> On the same delegated host class, Arm A runner-only and Arm C
> dual-capture had effectively identical median RSS. The RSS
> decomposition points to Runner capture as the memory-cost source.
> Wall-clock decomposition remains inconclusive: phase timing explains
> part of the Arm A / Arm C median gap, mostly around monitor attach, but
> the majority remains outside the current Runner phase buckets and Arm
> A's phase run had an unhealthy tail.

Next engineering slice:

> Do not add another broad Arm A/C wall-clock rerun for this arc. The
> paired residual diagnostic has landed and shows that the median gap is
> not stable enough for an additive wall-clock decomposition at the
> current measurement budget. Slice 10 smoke runs have now validated the
> event-rate / workload-intensity knobs on main. If the overhead arc
> continues, the next slice should be a predeclared small matrix that
> varies kernel-event rate, span/event rate, concurrency, and payload
> size, then reports slopes and thresholds rather than another single
> wall-clock delta.
