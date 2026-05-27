# Runner vs OTel Overhead Measurement Plan (2026-05)

> **Status:** measurement follow-up with Slices 1-9 complete and Slice 10
> smoke-verified. This
> document turns the explicit overhead non-claim from
> [`runner-vs-otel-2026-05`](runner-vs-otel-2026-05/) into a reproducible
> measurement plan and findings trail. It does not commit generated
> benchmark artifacts, does not publish cross-host deltas, and does not
> change Runner archive semantics.
>
> **Slice 1 status:** local Arm B harness and schema sidecars live under
> [`runner-vs-otel-overhead-2026-05/`](runner-vs-otel-overhead-2026-05/).
> Generated measurements are still not committed evidence.
>
> **Slice 2 status:** delegated Arm C workflow passed on
> [GitHub Actions run 26449999294](https://github.com/Rul1an/assay/actions/runs/26449999294):
> 20/20 valid samples, 0 discarded samples, all Runner health gates
> clean. The uploaded artifacts remain review artifacts and are not
> committed benchmark numbers.
>
> **Slice 3 status:** delegated Arm C RSS workflow passed on
> [GitHub Actions run 26454010701](https://github.com/Rul1an/assay/actions/runs/26454010701):
> 5/5 valid samples, 0 discarded samples, all Runner health gates clean,
> peak RSS median 116,649,984 bytes, max 116,781,056 bytes. The uploaded
> artifacts remain review artifacts and are not committed benchmark
> numbers.
>
> **Slice 4 status:** the harness emits schema-validated `summary.json`,
> reviewer-friendly `summary.md`, and derived BMF metrics. This is still
> per-arm baseline reporting; findings must not present cross-host deltas.
>
> **Slice 5 status:** initial findings are summarized in
> [`runner-vs-otel-overhead-2026-05/findings.md`](runner-vs-otel-overhead-2026-05/findings.md).
> The initial result was an Arm C host-class baseline.
>
> **Slice 6 status:** same-host Arm B passed on
> [GitHub Actions run 26459699303](https://github.com/Rul1an/assay/actions/runs/26459699303)
> for wall-clock (20/20 valid, 0 discarded) and
> [GitHub Actions run 26461726436](https://github.com/Rul1an/assay/actions/runs/26461726436)
> for RSS (5/5 valid, 0 discarded). Both emitted the same
> `linux-aarch64-6.8.0-117-generic` host class as Arm C, so the findings
> document now reports a narrow same-host delta for this deterministic
> workload.
>
> **Slice 7 status:** optional Arm A (`arm-a-runner-only`) passed on
> [GitHub Actions run 26463798358](https://github.com/Rul1an/assay/actions/runs/26463798358)
> for wall-clock (20/20 valid, 0 discarded) and
> [GitHub Actions run 26464003194](https://github.com/Rul1an/assay/actions/runs/26464003194)
> for RSS (5/5 valid, 0 discarded). Both emitted the same
> `linux-aarch64-6.8.0-117-generic` host class as Arm B and Arm C. Arm A
> is only for decomposing the current Arm C delta into "Runner archive
> only" versus "Runner archive plus OTel trace"; it is not a new product
> benchmark.
>
> **Slice 8 status:** phase timing dispatched. A diagnostic repeat of Arm A wall-clock
> ([GitHub Actions run 26472122983](https://github.com/Rul1an/assay/actions/runs/26472122983))
> failed because one sample was discarded. The temporary runner workspace
> showed the same first-sample cgroup spawn failure pattern seen during
> the original sanity attempt. The workflow now uploads partial artifacts
> even when the harness exits non-zero. A follow-up repeat
> ([GitHub Actions run 26473448298](https://github.com/Rul1an/assay/actions/runs/26473448298))
> passed with 20/20 valid samples and a healthy p99/median ratio, but
> Arm A remained slower than Arm C at the median. Phase-timing runs
> [26476490968](https://github.com/Rul1an/assay/actions/runs/26476490968)
> (Arm A) and
> [26476824593](https://github.com/Rul1an/assay/actions/runs/26476824593)
> (Arm C) then passed with 20/20 valid samples each. They explain only
> part of the median wall-clock gap, mostly around monitor attach, so the
> findings still withhold an additive wall-clock decomposition claim.
>
> **Slice 9 status:** paired Arm A/C residual diagnostics passed on
> [GitHub Actions run 26479319306](https://github.com/Rul1an/assay/actions/runs/26479319306):
> 20 adjacent counterbalanced pairs, 20/20 valid samples per arm, 0
> discarded, same host class. The paired run shows the Slice 8 Arm A-over-Arm C
> median gap does not reproduce under adjacent pairing; wall-clock
> decomposition remains unpublished and should stop at this measurement
> budget.
>
> **Slice 10 status:** smoke-verified. The overhead workflow accepts
> controlled kernel-event rate, span/event rate, concurrency, and payload
> size inputs. Two small paired A/C smoke dispatches passed on main:
> [26508127380](https://github.com/Rul1an/assay/actions/runs/26508127380)
> (`kernel=low`, `span=baseline`, `concurrency=1`) and
> [26508355816](https://github.com/Rul1an/assay/actions/runs/26508355816)
> (`kernel=medium`, `span=low`, `concurrency=2`). Both had 2/2 valid
> samples per arm, 0 discarded samples, clean Runner health gates, and
> matching host class. These are workflow and metadata smoke checks, not
> published sweep measurements.

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
  arm-a-runner-only/
    samples.jsonl
    summary.json
  arm-b-otel/
    samples.jsonl
    summary.json
  arm-c-dual-capture/
    samples.jsonl
    summary.json
  artifacts/
    trace-sizes.json
    archive-sizes.json
    phase-timings.json
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
  "phase_timings_ms": {
    "preflight_ms": 0.1,
    "cgroup_prepare_ms": 1.2,
    "monitor_attach_ms": 3.4,
    "child_spawn_ms": 5.6,
    "child_runtime_ms": 789.0,
    "event_flush_ms": 100.0,
    "archive_write_ms": 12.3
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
  },
  "phase_timings_ms": {
    "child_runtime_ms": {
      "median": 0,
      "p95": 0,
      "p99": 0
    }
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
  "runner_vs_otel.arm_c_dual_capture.wall_clock_ms.median": { "value": 0 },
  "runner_vs_otel.arm_c_dual_capture.wall_clock_ms.p99": { "value": 0 },
  "runner_vs_otel.arm_c_dual_capture.peak_rss_bytes.max": { "value": 0 }
}
```

Phase timing metrics, when present, use the same derived BMF convention,
for example
`runner_vs_otel.arm_c_dual_capture.phase_timings_ms.child_runtime_ms.median`.
They are diagnostics for this experiment and do not replace
`wall_clock_ms`.

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

Arm A stays out of the headline delta unless Arm C overhead needs
decomposition into "Runner archive only" versus "Runner archive plus
OTel trace". It uses the same sample-count, health, and provenance gates
as Arm C.

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

Linux RSS collection requires GNU `/usr/bin/time -v` (time 1.7 or
newer). BusyBox or Alpine-style `time` output is not accepted by the v0
parser and should fail the sample rather than produce a silent null.

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

## Phase-Timing Follow-up

The same-host results make RSS attribution clear enough. Slice 8 added
phase timing because wall-clock remained too coarse for an additive
decomposition claim.

Required phase buckets:

| Phase | Question | Expected source |
|---|---|---|
| `preflight_ms` | Is host/tooling preflight or per-run directory setup visible in the sample? | overhead harness |
| `cgroup_prepare_ms` | Does cgroup domain-root resolution or session setup dominate? | runner-spike / `assay-runner-linux` |
| `monitor_attach_ms` | Does eBPF/LSM/tracepoint attach dominate? | runner-spike + monitor adapter |
| `child_spawn_ms` | Is process placement/spawn the failure or tail source? | runner-spike / cgroup placement |
| `child_runtime_ms` | How long does the deterministic fixture itself run? | runner-spike child wait |
| `event_flush_ms` | Does SDK/kernel event flush add tail latency? | runner-spike archive assembly |
| `archive_write_ms` | Does tar/gzip or layer materialization dominate? | runner-spike archive assembly |
| `health_parse_ms` | Does post-run health/correlation parsing add measurable cost? | overhead harness follow-up if extraction proves material |

Acceptance rules for this slice:

- Emit phase timings as experiment-scoped diagnostics, not Runner archive
  evidence, unless a later contract explicitly promotes them.
- Keep raw end-to-end `wall_clock_ms`; phase timings are explanatory
  projections and must not replace the sample timing.
- Upload partial artifacts when the harness fails, because discarded
  samples and cgroup errors are the evidence needed for diagnosis.
- Add a one-sample warmup option only after phase data confirms whether
  the first-sample cgroup spawn failure is a warmup artifact rather than
  a correctness bug.
- Do not publish a wall-clock additive split until phase data explains
  why the healthy Arm A repeat remains slower than Arm C at the median,
  or shows that the gap lives outside the instrumented Runner phases.

Slice 8 result:

- Arm A phase timing
  ([run 26476490968](https://github.com/Rul1an/assay/actions/runs/26476490968)):
  20/20 valid, 0 discarded, same host class, but unhealthy tail
  (`p99/median=3.619`).
- Arm C phase timing
  ([run 26476824593](https://github.com/Rul1an/assay/actions/runs/26476824593)):
  20/20 valid, 0 discarded, same host class, healthy tail
  (`p99/median=1.153`).
- Summed phase medians explain `34.540 ms` of the `107.251 ms` Arm A
  over Arm C median wall-clock gap. The largest instrumented phase delta
  is `monitor_attach_ms` (`+38.284 ms` for Arm A), while the wall median
  minus summed phase medians leaves `72.711 ms` outside the timed phase
  buckets.

## Residual Diagnostics Follow-up

Slice 9 is the next useful experiment if the wall-clock question still
matters. It should measure Arm A and Arm C in one delegated workflow job
with adjacent, counterbalanced pairs (`A/C`, then `C/A`) and keep a
derived residual record for each sample:

```text
phase_residual_ms = wall_clock_ms - sum(recorded phase_timings_ms)
```

This is deliberately not a new benchmark claim. It asks whether the
unexplained `72.711 ms` median residual from Slice 8 is stable when arm
order and runner load drift are reduced, and whether the unexplained
time is tied to a specific pair/order position. Negative residuals are
allowed as diagnostics; they mean the timed phase sum exceeded the outer
wall-clock sample, usually because of clock asymmetry, overlapping phase
boundaries, or measurement noise. They are not publishable overhead
quantities by themselves.

Pre-read and rationale:

- Distributed tracing overhead is known to vary by workload,
  configuration, and deployment environment. Nõu et al. report
  throughput and latency impacts for OpenTelemetry/Elastic APM across
  microservice and serverless workloads, and identify trace
  serialization/export as a major source of overhead:
  <https://doi.org/10.1145/3680256.3721316>.
- The OpenTelemetry benchmark guidance frames overhead as
  target-platform specific and separates span/instrumentation cost,
  throughput, CPU, memory, and report shape:
  <https://opentelemetry.io/docs/specs/otel/performance-benchmark/>.
- BPF/eBPF overhead measurement is itself hard to isolate. Red Hat's BPF
  performance guide calls out "who traces the tracer?" as the core
  problem and emphasizes measuring the right hook/attach path:
  <https://developers.redhat.com/articles/2022/06/22/measuring-bpf-performance-tips-tricks-and-best-practices>.
- Observability-overhead noise is a known phenomenon. Reichelt, Jung,
  and van Hoorn compare MooBench across GitHub Actions and bare-metal
  environments and show that shared/cloud execution noise affects what
  changes are detectable:
  <https://arxiv.org/abs/2411.05491>.

Acceptance rules for Slice 9:

- Dispatch `arm=paired-a-c` only on `assay-bpf-runner`, with
  `repetitions=20`, `measure_rss=false`, and phase timing enabled by the
  Runner harness path.
- Treat each repetition as a pair, not as independent arm samples. The
  harness order is counterbalanced adjacent pairs: odd pairs run Arm A
  then Arm C; even pairs run Arm C then Arm A.
- Publish no additive wall-clock decomposition unless the paired
  residuals either shrink below the existing unexplained gap or point to
  a repeatable order/phase source.
- If the residual remains material and order-independent, close the
  wall-clock decomposition as "not additively decomposable at this
  measurement budget" rather than adding another broad rerun.

Decision tree after dispatch:

- **Residual shrinks materially under pairing:** treat Slice 8 as
  inflated by inter-dispatch drift and update findings with the paired
  caveat.
- **Residual is order-dependent:** file a narrow warmup/cache/order slice
  instead of attributing the gap to capture mode.
- **Residual remains material and order-independent:** stop the
  wall-clock decomposition arc at the current measurement budget.

Slice 9 result:

- Paired Arm A/C residual diagnostics
  ([run 26479319306](https://github.com/Rul1an/assay/actions/runs/26479319306)):
  20 adjacent counterbalanced pairs in one delegated job, 20/20 valid
  samples per arm, 0 discarded, same host class.
- The Slice 8 Arm A-over-Arm C median wall-clock gap did not reproduce
  under pairing. Arm A was `111.074 ms` faster than Arm C at the median
  in the paired run, while median per-sample residuals differed by only
  `22.476 ms`.
- Both paired tails were unhealthy (`p99/median=2.166` for Arm A and
  `2.295` for Arm C), so the result does not publish a new additive
  split. It closes the broad wall-clock decomposition loop as unstable
  at n=20 on this delegated runner. RSS remains the clean decomposition
  signal.

## Event-Rate Sweep Follow-up

Slice 10 is the next useful experiment if we want one more overhead
slice. It is a controlled event-rate and workload-intensity sweep, not
another broad Arm A/C wall-clock repeat.

The question changes from:

```text
Is Arm A faster or slower than Arm C?
```

to:

```text
At what kernel-event rate, span/event rate, and concurrency level does
Runner or OTel overhead become measurable, and which component scales?
```

Rationale from the literature:

- OpenTelemetry's benchmark guidance frames overhead as target-platform
  and event-throughput specific, not as a universal constant:
  <https://opentelemetry.io/docs/specs/otel/performance-benchmark/>.
- Nõu et al. show distributed-tracing overhead varies by workload,
  endpoint, framework, and deployment shape:
  <https://doi.org/10.1145/3680256.3721316>.
- Reichelt, Jung, and van Hoorn show that shared/cloud runner noise can
  hide or distort observability-overhead deltas:
  <https://arxiv.org/abs/2411.05491>.
- Red Hat's BPF performance guide warns that measuring tracing overhead
  can itself perturb the thing being measured:
  <https://developers.redhat.com/articles/2022/06/22/measuring-bpf-performance-tips-tricks-and-best-practices>.
- Recent eBPF component-analysis work decomposes tracing overhead by
  event-generation and user-space retrieval stages, and reports that the
  overhead is workload- and event-rate dependent:
  <https://doi.org/10.1145/3805687.3806254>.

Proposed independent variables:

| Axis | Levels | Purpose |
|---|---|---|
| Kernel-event rate | low / medium / high | Does eBPF capture cost scale with syscall/event pressure? |
| Span/event rate | low / medium / high | Does trace export cost scale separately from kernel capture? |
| Concurrency | 1 / 4 / 16 workers | Does overhead remain sequential or become scheduling/noise dominated? |
| Payload size | small / medium / large | Separates event count from bytes moved and archive/trace size. |

Proposed arms:

- **Arm A:** Runner archive capture only.
- **Arm B:** OTel/OpenInference-style tracing only.
- **Arm C:** Runner archive capture plus OTel trace export.

Arm A cannot apply OTel span/event pressure because it deliberately has
no trace export. If a paired dispatch requests non-baseline span/event
pressure, Arm A samples record `span_event_rate=baseline` and
`target_span_events=0`; Arm C records and applies the requested target.

Metrics:

- `wall_clock_ms` as a secondary metric only;
- `peak_rss_bytes`;
- `phase_timings_ms` and `phase_residual_ms`;
- kernel event count per sample;
- span/event count per sample;
- ring-buffer drops and Runner health gates;
- trace JSON size, archive compressed size, archive extracted size;
- optional CPU-time or hardware/software counter data if it can be
  collected without perturbing the workload more than the signal.

Acceptance rules for Slice 10:

- Start with a harness-only PR. Do not commit sweep measurements in the
  first slice.
- Keep output experiment-scoped. If a new artifact is introduced, use an
  `assay.experiment.event_rate_sweep.v0` schema string and sidecar tests.
- Use paired/counterbalanced order inside one delegated job whenever two
  arms are compared.
- Publish no product benchmark. Report slopes and thresholds, such as
  "overhead per 1k kernel events" or "ring-buffer retrieval becomes
  visible above this event rate," only with host class and workload
  caveats.
- Stop if tails fail the existing p99/median health band; do not rescue
  the result by picking only favorable levels.

Smoke status:

- Run
  [26508127380](https://github.com/Rul1an/assay/actions/runs/26508127380)
  verified the post-merge paired A/C path with `kernel=low`,
  `span=baseline`, `concurrency=1`, and `payload=small`.
- Run
  [26508355816](https://github.com/Rul1an/assay/actions/runs/26508355816)
  verified the non-baseline span/event metadata and `concurrency=2`
  kernel-pressure path with `kernel=medium`, `span=low`,
  `concurrency=2`, and `payload=small`.
- The second smoke produced `assay.sweep.*` trace attributes and
  `assay.sweep.span_event` events in Arm C, while Arm A correctly
  recorded `span_event_rate=baseline` and `target_span_events=0`.
- Both smoke runs remain review artifacts only. The first real findings
  slice should still use a predeclared small matrix and report slopes or
  thresholds, not these n=2 smoke medians.

Slice 11 starter matrix:

The first real sweep should stay small enough to inspect manually and
large enough to separate "knob works" from "signal exists." Use paired
A/C dispatches, `measure_rss=false`, `build_ebpf=true`,
`timeout_seconds=300`, and `repetitions=5` for each cell. Do not add
Arm B or RSS until these cells pass health gates.

| Cell | Kernel-event rate | Span/event rate | Concurrency | Payload | Purpose |
|---|---|---|---:|---|---|
| control | `baseline` | `baseline` | `1` | `small` | Same workflow shape with no sweep pressure |
| kernel-high | `high` | `baseline` | `1` | `small` | Is eBPF/kernel-event pressure visible by itself? |
| span-high | `baseline` | `high` | `1` | `small` | Is OTel span-event pressure visible by itself? |
| kernel-concurrent | `high` | `baseline` | `4` | `small` | Does kernel pressure change under modest concurrency? |
| corner | `high` | `high` | `4` | `large` | First combined stress corner for drops, trace growth, and archive growth |

The smoke dispatches verify environment and CLI propagation only. They
do not verify rate calibration, payload-size behavior, or high
concurrency. The Slice 11 analysis must therefore check observed kernel
event paths and Arm C trace event counts against the declared targets in
each cell before interpreting timing.

Slice 11 acceptance rules:

- Publish no slope or threshold unless every reported cell has 5/5 valid
  samples per arm, 0 discarded samples, `ringbuf_drops=0`,
  `kernel_layer=complete`, and `cgroup_correlation=clean`.
- Treat the `corner` cell as a threshold probe. If it produces drops,
  trace truncation, or unhealthy tails, report the failure boundary
  instead of dropping the cell.
- If all starter cells are healthy but show no measurable signal, do not
  keep broad-rerunning. Either widen the event-rate levels in a new
  predeclared slice or close the sweep as "no visible signal at the
  starter matrix budget."
- Keep artifacts review-only until a findings PR decides which summary
  tables should become committed evidence.

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
| 2 | **Done**: delegated Arm C harness with health-gated samples via [`.github/workflows/runner-otel-overhead-experiment.yml`](../../.github/workflows/runner-otel-overhead-experiment.yml) | n=20 on `assay-bpf-runner`, all health gates clean |
| 3 | **Done**: RSS collection per arm via `--measure-rss` / workflow `measure_rss=true` | n=5 on `assay-bpf-runner`, platform-specific parser tests, tool versions recorded per sample |
| 4 | **Done**: summary renderer + BMF-compatible export | JSON schema-like tests over synthetic samples |
| 5 | **Done**: initial findings update in [`runner-vs-otel-overhead-2026-05/findings.md`](runner-vs-otel-overhead-2026-05/findings.md) | No deltas unless same-host arms exist |
| 6 | **Done**: same-host Arm B delegated workflow path via `arm=arm-b-otel`, dispatched in runs [26459699303](https://github.com/Rul1an/assay/actions/runs/26459699303) and [26461726436](https://github.com/Rul1an/assay/actions/runs/26461726436) | n=20 wall-clock and n=5 RSS on `assay-bpf-runner`; `host_class` matches Arm C |
| 7 | **Done**: Arm A pure-L2 decomposition via `arm=arm-a-runner-only`, dispatched in runs [26463798358](https://github.com/Rul1an/assay/actions/runs/26463798358), [26464003194](https://github.com/Rul1an/assay/actions/runs/26464003194), and healthy repeat [26473448298](https://github.com/Rul1an/assay/actions/runs/26473448298) | RSS decomposition landed; wall-clock decomposition remains inconclusive because Arm A is still slower than Arm C at the median |
| 8 | **Done**: Runner phase timing via hidden `--phase-timing-log` and harness `phase_timings_ms` aggregation, dispatched in runs [26476490968](https://github.com/Rul1an/assay/actions/runs/26476490968) and [26476824593](https://github.com/Rul1an/assay/actions/runs/26476824593) | phase data explains part, not all, of the Arm A / Arm C median gap; no additive wall-clock decomposition claim |
| 9 | **Done**: paired Arm A/C residual diagnostics via workflow `arm=paired-a-c`, dispatched in run [26479319306](https://github.com/Rul1an/assay/actions/runs/26479319306) | residuals shrink/change sign under pairing; wall-clock decomposition remains unpublished and should stop at this measurement budget |
| 10 | **Smoke-verified**: controlled event-rate / workload-intensity sweep via workflow inputs and sample/summary metadata, with paired smoke runs [26508127380](https://github.com/Rul1an/assay/actions/runs/26508127380) and [26508355816](https://github.com/Rul1an/assay/actions/runs/26508355816) | no broad rerun; dispatch only a small matrix first, with kernel-event count, span/event count, concurrency, phase timing, residual, RSS, and health gates reported by level |
| 11 | **Planned**: predeclared Slice 10 starter matrix with five paired A/C cells: control, kernel-high, span-high, kernel-concurrent, and corner | n=5 paired samples per cell; observed event counts match targets; publish only slopes/thresholds with health gates, never n=2 smoke medians |

## Publication Rule

Publication may mention the same-host Arm B-vs-Arm C delta only with the
host-class and non-decomposition caveats from the findings document. Do
not present it as a product benchmark, model/provider latency claim, or
co-temporal variance result. Do not add overhead numbers to the
OpenInference discussion unless that distinction is explicit in the
wording.
