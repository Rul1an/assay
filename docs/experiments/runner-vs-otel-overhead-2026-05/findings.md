# Runner vs OTel Overhead Findings (2026-05)

> **Status:** same-host findings update. This document summarizes the
> overhead follow-up evidence collected so far. It does not commit the
> generated measurement artifacts. Direct Arm B-vs-Arm C deltas are
> reported only for the matching `linux-aarch64-6.8.0-117-generic` host
> class.

## Evidence Anchors

| Slice | Workflow run | Arm | Samples | Result |
|---|---|---|---:|---|
| 2 | [26449999294](https://github.com/Rul1an/assay/actions/runs/26449999294) | Arm C dual capture | 20 wall-clock | 20 valid, 0 discarded, all health gates clean |
| 3 | [26454010701](https://github.com/Rul1an/assay/actions/runs/26454010701) | Arm C dual capture | 5 RSS | 5 valid, 0 discarded, all health gates clean |
| 6 wall | [26459699303](https://github.com/Rul1an/assay/actions/runs/26459699303) | Arm B OTel-only | 20 wall-clock | 20 valid, 0 discarded, same host class |
| 6 RSS | [26461726436](https://github.com/Rul1an/assay/actions/runs/26461726436) | Arm B OTel-only | 5 RSS | 5 valid, 0 discarded, same host class |

Generated artifacts from those runs were inspected as review artifacts
only. They are intentionally not committed as benchmark evidence in this
slice.

## Same-Host Baselines

Both arms now have clean measurements on the same delegated
`assay-bpf-runner` host class:

| Metric | Value | Interpretation |
|---|---:|---|
| Host class | `linux-aarch64-6.8.0-117-generic` | Delegated Linux runner machine/OS/kernel boundary |
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
| Arm C trace JSON median | `3,220 bytes` | L1 trace plus Runner wrapper footprint |
| Arm C archive `.tar.gz` median | `1,776-1,777 bytes` | L2 compressed archive footprint baseline |
| Arm C archive extracted median | `8,186-8,187 bytes` | Review/storage footprint baseline |

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
around the deterministic workload. It does not decompose how much of
that cost is pure Runner archive capture versus coordination around the
OTel trace; optional Arm A remains the path for that question.

## What This Means

- The delegated measurement harness is usable for both arms: wall-clock
  and RSS runs produced all-valid samples on the same host class.
- The observed Arm C tail ratio is healthy for this deterministic
  workload on `assay-bpf-runner`.
- The observed Arm C median wall-clock is about 2x Arm B on the same
  host class for this workload, while RSS increases by about 7%.
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
  host class at different times.
- No decomposition claim is made between "Runner archive only" and
  "Runner archive plus OTel trace".
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

Optional Arm A remains deferred unless Arm C needs decomposition into
"Runner archive only" versus "Runner archive plus OTel trace".
