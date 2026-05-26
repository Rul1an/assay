# Runner vs OTel Overhead Measurement Plan (2026-05)

> **Status:** plan-only follow-up. This document turns the explicit
> overhead non-claim from
> [`runner-vs-otel-2026-05`](runner-vs-otel-2026-05/) into a reproducible
> measurement plan. It does not add live measurements, does not publish a
> benchmark claim, and does not change Runner archive semantics.
>
> **Slice 1 status:** local Arm B harness and schema sidecars live under
> [`runner-vs-otel-overhead-2026-05/`](runner-vs-otel-overhead-2026-05/).
> Generated measurements are still not committed evidence.
>
> **Slice 2 status:** delegated Arm C workflow is available as
> [`.github/workflows/runner-otel-overhead-experiment.yml`](../../.github/workflows/runner-otel-overhead-experiment.yml).
> It uploads review artifacts and still does not commit benchmark
> numbers.

## Research Question

What wall-clock, memory, and artifact-size overhead does each observation
mode add around the same deterministic agent workload?

The answer must stay scoped to the measurement boundary:

- **Arm B:** in-process OTel/OpenInference-style tracing only;
- **Arm C:** OTel trace plus Runner archive capture using Linux/eBPF +
  cgroup-v2 on `assay-bpf-runner`;
- **optional Arm A:** Runner archive capture only, if we need a pure L2
  comparison against Arm C.

This is not a model-quality benchmark, not a runtime ranking, and not a
claim that a local developer machine and the delegated Linux runner are
directly comparable.

## Existing Evidence Boundary

The main experiment already proves shape claims:

- per-run manifest binding;
- tool-level `gen_ai.tool.call.id` join;
- controlled reported-intent vs measured-effect mismatch;
- operation-aware kernel-event evidence after the kernel-event v0 rerun.

It explicitly does **not** prove overhead. The existing `n=3` runs are
shape-stability samples. They must not be reused as latency samples.

## Metrics

| Metric | Sample | Purpose | Output |
|---|---:|---|---|
| End-to-end wall clock | n >= 20 per arm | Capture overhead | median, p95, p99, p99/median |
| Peak RSS | n >= 5 per arm | Memory overhead | median, max |
| Trace export size | n = 3 | L1 storage footprint | bytes |
| Archive compressed size | n = 3 | L2 storage footprint | bytes |
| Archive extracted size | n = 3 | Review/storage footprint | bytes |
| Measurement health | every Arm C/A run | Validity gate | `ringbuf_drops=0`, `kernel_layer=complete`, `cgroup_correlation=clean` |

Wall-clock and RSS are separate measurements. Do not infer RSS from
wall-clock runs unless the harness records both from the same process
tree reliably.

## Measurement Principles

1. **Same workload, different boundary.** The workload code and prompt
   stay fixed; only the observation layer changes.
2. **Warm build, cold run artifacts.** Build TypeScript and Rust once
   before timing. Each measured iteration gets a fresh run directory.
3. **No live model variance.** Prefer the deterministic cassette/stub
   provider already used by the experiment package. If a live provider is
   unavoidable, record that the run is exploratory and not benchmarkable.
4. **Health-gated samples only.** A Runner sample with degraded kernel
   capture or ringbuf drops is discarded and rerun, not averaged in.
5. **Separate host classes.** Arm B local timings and Arm C delegated
   timings are not directly comparable unless they run on the same host.
6. **Report distributions.** Median-only is insufficient; p95 and p99
   are required because capture systems can add tail latency.
7. **BMF export path.** Keep `samples.jsonl` and `summary.json` in the
   experiment-scoped schemas below, then emit a separate Bencher Metric
   Format export whose top-level metric keys map to `{ "value": ... }`
   objects. The experiment schemas are not directly ingestible by
   Bencher.

## Harness Shape

The harness should produce one directory per arm:

```text
runs/overhead-2026-05/
  arm-b-otel/
    samples.jsonl
    summary.json
  arm-c-dual-capture/
    samples.jsonl
    summary.json
  artifacts/
    trace-sizes.json
    archive-sizes.json
    bmf.json
```

Each line in `samples.jsonl` should be a self-contained measurement:

```json
{
  "schema": "assay.experiment.overhead_sample.v0",
  "experiment": "runner-vs-otel-overhead-2026-05",
  "arm": "arm-c-dual-capture",
  "iteration": 1,
  "host": "assay-bpf-runner",
  "host_class": "assay-bpf-runner-linux-arm64-kernel-6.8",
  "assay_commit": "ee343650",
  "started_at": "2026-05-26T00:00:00Z",
  "tool_versions": {
    "hyperfine": "1.19.0",
    "time": "GNU time 1.9",
    "node": "v22.16.0"
  },
  "wall_clock_ms": 1234.5,
  "peak_rss_bytes": 123456789,
  "exit_code": 0,
  "health": {
    "kernel_layer": "complete",
    "ringbuf_drops": 0,
    "cgroup_correlation": "clean"
  },
  "artifact_bytes": {
    "trace_json": 12345,
    "archive_targz": 67890,
    "archive_extracted": 234567
  }
}
```

`summary.json` should aggregate only valid samples:

```json
{
  "schema": "assay.experiment.overhead_summary.v0",
  "experiment": "runner-vs-otel-overhead-2026-05",
  "arm": "arm-c-dual-capture",
  "host": "assay-bpf-runner",
  "host_class": "assay-bpf-runner-linux-arm64-kernel-6.8",
  "kernel": "6.8.0-117-generic",
  "assay_commit": "ee343650",
  "delegated_workflow_url": "https://github.com/Rul1an/assay/actions/runs/123456789",
  "valid_samples": 20,
  "discarded_samples": 0,
  "wall_clock_ms": {
    "median": 0,
    "p95": 0,
    "p99": 0,
    "p99_over_median": 0
  },
  "peak_rss_bytes": {
    "median": 0,
    "max": 0
  },
  "artifact_bytes": {
    "trace_json_median": 0,
    "archive_targz_median": 0,
    "archive_extracted_median": 0
  }
}
```

`archive_extracted` records the byte size of the extracted archive
directory for the same sample. `archive-sizes.json` may duplicate these
values for convenience, but the per-sample field is the source of truth
for aggregation.

The BMF export is a derived artifact, for example:

```json
{
  "runner_vs_otel.arm_c.wall_clock_ms.median": { "value": 0 },
  "runner_vs_otel.arm_c.wall_clock_ms.p99": { "value": 0 },
  "runner_vs_otel.arm_c.peak_rss_bytes.max": { "value": 0 }
}
```

The JSON shape is intentionally experiment-scoped. It is not a Runner
archive contract. The `assay.experiment.*` namespace is reserved for
time-limited experiment evidence that may change between experiment
slices. It must not be treated as a stable Runner archive namespace
unless a later reference document explicitly promotes it.

Slice 1 must add JSON Schema sidecars for these two shapes, for example
`schema/overhead-sample-v0.schema.json` and
`schema/overhead-summary-v0.schema.json`, plus sidecar tests that
validate emitted synthetic samples. This prevents the harness output and
the documented shape from drifting apart once code starts emitting data.

## Arm Definitions

| Arm | Host | Command Shape | Required Sample |
|---|---|---|---:|
| B | local or delegated same-host | `node workload.js --trace-out ...` | n >= 20 wall-clock, n >= 5 RSS |
| C | `assay-bpf-runner` | `assay runner-spike run --kernel-capture --ebpf ... -- node workload.js ...` | n >= 20 wall-clock, n >= 5 RSS |
| A optional | `assay-bpf-runner` | `assay runner-spike run --kernel-capture --ebpf ... -- node fixture-agent.js` | n >= 20 wall-clock, n >= 5 RSS |

If Arm B is measured locally and Arm C is measured on the delegated
runner, report them as separate host-class baselines. A direct delta
requires Arm B to run on the delegated runner too. `host_class` is the
mechanical comparison key for this rule. It should be a stable label for
the hardware/OS/kernel boundary, not a free-form display name.

Arm A stays out of the v0 sequence unless Arm C overhead needs
decomposition into "Runner archive only" versus "Runner archive plus
OTel trace". If needed, add it as a follow-up Slice 6 with the same
sample-count, health, and provenance gates.

## Tooling

Use existing Assay performance vocabulary where possible:

- `scripts/perf_e2e.sh` establishes the Hyperfine style: warmups,
  repeated runs, exported JSON, median and p95.
- `scripts/perf_assess.sh` establishes the BMF/summary discipline and
  the rule that performance claims need repeated samples.

The overhead harness may be a new script under the experiment directory
instead of extending the general Assay perf scripts. That keeps agent
observability overhead separate from store/SQLite performance.

Every emitted sample must record the measurement tool versions used to
produce it. This is required because `hyperfine` JSON, GNU time, and
macOS time expose different output shapes. Parser tests should assert
the exact formats accepted by the harness rather than assuming the host
tooling is interchangeable.

Preferred tools:

| Need | Linux | macOS |
|---|---|---|
| Wall clock | `hyperfine` or `/usr/bin/time -f %e` | `hyperfine` or `/usr/bin/time -p` |
| Peak RSS | `/usr/bin/time -v` (`Maximum resident set size`) | `/usr/bin/time -l` (`maximum resident set size`) |
| Artifact bytes | `stat -c %s` | `stat -f %z` |

## Acceptance Gates

| Gate | Requirement |
|---|---|
| Sample count | At least 20 valid wall-clock samples per reported arm |
| RSS count | At least 5 valid RSS samples per reported arm |
| Health | All Runner samples used in summary are capture-clean |
| Provenance | Summary records host, kernel, assay commit, workflow URL if delegated |
| Distribution | Summary reports median, p95, p99, p99/median |
| Non-claim | Report says whether arms ran on same host before presenting deltas |

Tail-ratio interpretation should reuse
[`docs/PERFORMANCE-ASSESSMENT.md`](../PERFORMANCE-ASSESSMENT.md) unless
the findings document explicitly says overhead measurements use a
different threshold model. For v0, `p99/median < 1.5` is healthy,
`1.5-2.0` is warning territory, and `> 2.0` is a fail signal requiring
investigation before publication.

## Non-Claims

- Does not rank OpenTelemetry, OpenInference, or Runner as products.
- Does not claim model/provider latency.
- Does not claim cross-host overhead deltas unless all compared arms ran
  on the same host.
- Does not turn overhead into a Trust Card or Trust Basis claim.
- Does not replace Criterion/Bencher store benchmarks.

## Suggested Slices

| Slice | Deliverable | Gate |
|---|---|---|
| 0 | This plan doc | Links from runner-vs-otel plan and README |
| 1 | **Done**: local harness for Arm B wall-clock + size output, plus `overhead-sample-v0` and `overhead-summary-v0` schema sidecars | n=20 local dry run, no live API dependency, sidecar tests pass |
| 2 | **Ready to dispatch**: delegated Arm C harness with health-gated samples via [`.github/workflows/runner-otel-overhead-experiment.yml`](../../.github/workflows/runner-otel-overhead-experiment.yml) | n=20 on `assay-bpf-runner`, all health gates clean |
| 3 | RSS collection per arm | n=5 per arm, platform-specific parser tests, tool versions recorded per sample |
| 4 | Summary renderer + BMF-compatible export | JSON schema-like tests over synthetic samples |
| 5 | Findings update | No deltas unless same-host arms exist |
| 6 optional | Arm A pure-L2 decomposition | Only if Arm C overhead needs archive-only vs dual-capture separation |

## Publication Rule

Do not add overhead numbers to the OpenInference discussion or blog until
Slices 1-5 have landed and the findings document can distinguish
same-host deltas from host-class baselines.
