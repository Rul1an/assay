# Published v6.22.0 proxy-cost observation — 2026-11-04

Three predeclared invocations of the existing Linux pilot completed against the
published v6.22.0 binary. This is a bounded single-host observation for #2494,
not a performance gate or a causal comparison with the October baseline.

Each invocation uses 20 alternating AB/BA blocks, 20 sequential calls per arm,
and a separate deny control. Values below are medians of paired block differences
(proxy minus direct), in milliseconds. Brackets are within-run bootstrap 95%
intervals from 2,000 resamples, not intervals across the three invocations.

| Metric | Run 1 | Run 2 | Run 3 |
| --- | --- | --- | --- |
| Startup/init/discovery | 4.469 [2.782, 31.424] | 4.234 [-11.525, 22.177] | 2.720 [0.950, 4.784] |
| Median call RTT | 0.563 [0.482, 0.649] | 0.526 [0.507, 0.559] | 0.556 [0.522, 0.575] |
| Total of 20 call RTTs | 17.249 [13.919, 37.728] | 14.260 [11.111, 22.733] | 12.176 [11.152, 12.770] |

Startup and total-call timings vary substantially, especially in the first two
runs. No sample was removed or replaced. This evidence does not support a stable
latency threshold or a claim that v6.22.0 is faster or slower than v6.14.1. The
historical baseline used another binary and time window on a shared host.

The allow evidence costs 21,500 bytes per 20-call session for this fixed input
(1,075 bytes per call). Closure medians are negative; the arms have different teardown paths and polling,
so these values should not be interpreted as speedups. RSS remains a non-comparable
single-process high-water observation, not aggregate process-tree memory.

## Identity and scope

- Measurement source: `55a8d106d007b078e54eecdbe1751ee76cedf923`.
  Its only harness behavior change makes release metadata explicit rather than
  always labeling an arbitrary binary as v6.14.1. Workload and statistics are unchanged.
- Release source: `d13a9bd7342bc3fbbd990acad27a0bd4be9963fb`, tag `v6.22.0`.
- Published archive: `assay-mcp-server-v6.22.0-aarch64-unknown-linux-gnu.tar.gz`,
  SHA256 `4d599877dc0fb894b54183ea524f2742425fcaa934b17aec4f49ac18eae35169`.
- Extracted binary SHA256:
  `d1726853202153c0bdd5f49b1b51593f8753d97d0591ac420f681033c6ee91eb`.
  The operator verified the archive attestation against the exact tag and source;
  the pilot itself does not authenticate those declarations.
- Linux `6.8.0-147-generic`, aarch64, four logical CPUs, Python 3.12.3,
  Ubuntu VM on a shared macOS host. No continuous isolation is claimed.
- UTC windows: 06:49:22–06:49:31, 06:50:01–06:50:06 and 06:50:36–06:50:38.
  Thirty-second pauses separated invocations. No warmup, reboot or cache flush;
  cache state and other host activity are uncontrolled.
- All three invocations exited zero. Source and binary hashes matched before and
  after each run. No replacement attempts were made. Regression tests on the frozen
  source passed before measurement; they are not additional benchmark attempts.

No tail-latency SLA, network/model overhead, concurrent-client behavior, wrap/import
cost, combined memory, safe-agent or compliance conclusion follows from this pilot.
#2494 remains open for its broader measurement scope.

## Independent recount

A non-building reviewer independently reconciled all three runs: 120 timed
sessions, 2,400 timed toolcalls, three deny controls, artifact byte lengths and
all five metric arrays, medians and bootstrap intervals. The [review](v6.22.0-recount/independent-review.md)
and [derived results](v6.22.0-recount/independent-recount.json) retain its exact scope.
Reviewer identity is declared, not externally authenticated. The original archive
was not changed by the review.

## Retained data and replay

[Original measurement package](release-6.22.0-2026-11-04.tar.gz) and
[SHA256](release-6.22.0-2026-11-04.tar.sha256) retain the frozen plan, execution
receipts, source archive, environment snapshots and original caller/evidence bytes.
No release binary is bundled. Machine paths in historical receipts are provenance,
not portable replay commands. Preserve the archive before extracting into a new directory.

```bash
sha256sum -c release-6.22.0-2026-11-04.tar.sha256
mkdir retained-6.22.0
tar -xzf release-6.22.0-2026-11-04.tar.gz -C retained-6.22.0
mkdir retained-6.22.0/source
tar -xf retained-6.22.0/source.tar -C retained-6.22.0/source
```

To recount retained data without executing the benchmark (Python 3.12 or newer),
from this directory use a fresh extraction directory:

```bash
mkdir v6.22.0-recount/independent-recount
tar -xzf release-6.22.0-2026-11-04.tar.gz -C v6.22.0-recount/independent-recount
python3 v6.22.0-recount/independent-recount.py
```

The script rewrites the derived `v6.22.0-recount/independent-recount.json`; preserve
the original archive and run in a disposable checkout if keeping the worktree clean
matters. Do not run Python with optimizations that disable assertions. Retained
clock values can be recalculated, but the original clock readings cannot be recreated.

For a new measurement, download the named release archive, verify its GitHub
attestation against the tag and source above, and verify the extracted binary hash.
From the extracted source directory run the pilot with these release metadata
values and `--blocks 20 --calls-per-session 20 --timeout-seconds 30
--bootstrap-resamples 2000
--expected-binary-sha256 d1726853202153c0bdd5f49b1b51593f8753d97d0591ac420f681033c6ee91eb`,
plus an absolute binary path and a new output directory.
Declare the attempt budget and host conditions before running; do not reuse the
historical `execute.py`, which contains machine-specific paths.
