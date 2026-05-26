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

Generated artifacts from those runs were inspected as review artifacts
only. They are intentionally not committed as benchmark evidence in this
slice.

The failed diagnostic repeat is listed to explain the workflow-DX fix,
not to replace the successful Slice 7 findings. The follow-up repeat
passed with artifacts uploaded and the new artifact-success gate active.
Together, the repeats show that the first-sample cgroup failure is not
deterministic, while wall-clock decomposition still needs phase timing
before it can support an additive claim.

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
- Arm A's repeat wall-clock tail is healthy, but the runner-only median
  remains higher than Arm C, so wall-clock decomposition remains a
  caution, not a benchmark claim.
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
  archive only" and "Runner archive plus OTel trace". The healthy Arm A
  repeat still does not compose with Arm C in a way that supports that
  claim.
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

Arm A measurements have landed. The safe publication language is now:

> On the same delegated host class, Arm A runner-only and Arm C
> dual-capture had effectively identical median RSS. The RSS
> decomposition points to Runner capture as the memory-cost source.
> Wall-clock decomposition remains inconclusive because the healthy Arm A
> repeat is still slower than Arm C, so it should not be reported as an
> additive split.

Next engineering slice:

> Instrument Runner phase timing for cgroup setup, monitor attach, child
> spawn/runtime, event flush, archive write, and health parsing. Failed
> harness runs should upload partial artifacts so discarded samples can
> be inspected from GitHub rather than from a temporary runner workspace.
