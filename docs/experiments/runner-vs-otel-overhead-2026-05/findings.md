# Runner vs OTel Overhead Findings (2026-05)

> **Status:** Slice 5 findings update. This document summarizes the
> overhead follow-up evidence collected so far. It does not commit the
> generated measurement artifacts and does not publish cross-host
> overhead deltas.

## Evidence Anchors

| Slice | Workflow run | Arm | Samples | Result |
|---|---|---|---:|---|
| 2 | [26449999294](https://github.com/Rul1an/assay/actions/runs/26449999294) | Arm C dual capture | 20 wall-clock | 20 valid, 0 discarded, all health gates clean |
| 3 | [26454010701](https://github.com/Rul1an/assay/actions/runs/26454010701) | Arm C dual capture | 5 RSS | 5 valid, 0 discarded, all health gates clean |

Generated artifacts from those runs were inspected as review artifacts
only. They are intentionally not committed as benchmark evidence in this
slice.

## Arm C Host-Class Baseline

The delegated Runner capture path is now measured on the
`assay-bpf-runner` host class:

| Metric | Value | Interpretation |
|---|---:|---|
| Host class | `linux-aarch64-6.8.0-117-generic` | Delegated Linux/eBPF runner baseline |
| Wall-clock valid samples | `20/20` | Meets the n >= 20 gate |
| Wall median | `1,737.838 ms` | Baseline for Arm C on this host class |
| Wall p95 | `2,051.039 ms` | Tail sample remained close to median |
| Wall p99 | `2,070.354 ms` | Nearest-rank p99 for n=20 |
| Wall p99/median | `1.191` | Healthy per the v0 `< 1.5` tail-ratio band |
| RSS valid samples | `5/5` | Meets the n >= 5 gate |
| Peak RSS median | `116,649,984 bytes` | Memory baseline for Arm C on this host class |
| Peak RSS max | `116,781,056 bytes` | No large RSS outlier in the n=5 sample |
| Trace JSON median | `3,220 bytes` | L1 trace footprint baseline |
| Archive `.tar.gz` median | `1,776-1,777 bytes` | L2 compressed archive footprint baseline |
| Archive extracted median | `8,186-8,187 bytes` | Review/storage footprint baseline |

The one-byte artifact-size spread between Slice 2 and Slice 3 is
expected for freshly generated archives and traces. The useful claim is
that the footprint is tiny and stable at this workload scale, not that
archive bytes are deterministic across runs.

## What This Means

- The delegated Arm C measurement harness is usable: both wall-clock and
  RSS runs produced all-valid samples with clean Runner health gates.
- The observed Arm C tail ratio is healthy for this deterministic
  workload on `assay-bpf-runner`.
- The RSS path works on the delegated Linux runner with GNU
  `/usr/bin/time -v`; samples record the RSS tool version and emit
  `peak_rss_bytes` into both `summary.json` and the BMF export.
- The summary renderer now gives reviewers the same metrics in
  `summary.md` and in the GitHub step summary, while `summary.json`
  remains canonical.

## What This Does Not Mean

- No direct Arm B-vs-Arm C overhead delta is reported here. Arm B was
  not measured on the same `linux-aarch64-6.8.0-117-generic` host class.
- No product ranking is implied between OpenTelemetry, OpenInference, or
  Assay-Runner.
- No model/provider latency claim is made. The workload is deterministic
  and measurement-scoped.
- No Trust Card or Trust Basis claim is added. This remains an
  experiment-scoped measurement follow-up.
- The generated artifacts remain review artifacts until a later decision
  explicitly promotes a measurement bundle into committed evidence.

## Next Work

The next useful measurement step is a same-host Arm B run on
`assay-bpf-runner`. The delegated workflow now has an `arm=arm-b-otel`
path for that purpose. To unblock a narrow Arm B-vs-Arm C delta, collect:

- Arm B wall-clock: `repetitions=20`, `measure_rss=false`;
- Arm B RSS: `repetitions=5`, `measure_rss=true`;
- matching `host_class` values between the Arm B and Arm C summaries.

Until then, the correct publication language is:

> Arm C dual capture has a clean delegated host-class baseline. Direct
> overhead deltas are still withheld because same-host Arm B data has not
> landed.

Optional Arm A remains deferred unless Arm C needs decomposition into
"Runner archive only" versus "Runner archive plus OTel trace".
