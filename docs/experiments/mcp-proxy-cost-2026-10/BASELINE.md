# Proxy-cost repeatability baseline — 2026-10-18

Three new invocations of the unchanged pilot completed on one shared Linux VM.
This is a short-window descriptive baseline for [#2494](https://github.com/Rul1an/assay/issues/2494),
not a performance gate. The earlier development pilot is excluded from this series.

## Results

Each invocation contains 20 alternating AB/BA blocks, with 20 sequential calls per
arm. Values below are medians of paired block differences (proxy minus direct),
in milliseconds. Brackets contain the existing within-run bootstrap 95% interval
(2,000 resamples); they are not confidence intervals across the three runs.

| Metric | Run 1 | Run 2 | Run 3 |
|---|---:|---:|---:|
| Startup/init/discovery | 2.012 [1.162, 3.304] | 2.212 [1.535, 3.213] | 3.093 [1.114, 4.252] |
| Median call RTT | 0.562 [0.508, 0.590] | 0.565 [0.529, 0.597] | 0.565 [0.519, 0.582] |
| Total of 20 call RTTs | 11.538 [10.769, 11.857] | 11.956 [11.062, 12.721] | 11.473 [10.591, 11.831] |

The three call-run medians span 0.562314–0.564877 ms; startup-run medians span
2.011964–3.093113 ms. Call medians were close during this series; startup varied
more. Interval overlap is not an equivalence test. No calls are pooled across runs
and no alert threshold is inferred.

Every run retained 400 direct calls, 400 proxy calls, 400 allow records and
883 caller responses, including a separate deny control. Each deny control observed
initialization/discovery and zero forwarded tool calls. Allow evidence occupied
430,000 bytes per run: 21,500 bytes per session, or 1,075 bytes per call for this
fixed input.

Closure medians were -1.968, -1.774 and -1.778 ms. Different teardown paths and
polling prevent interpreting these as a speedup. RSS remains a non-comparable
single-process high-water observation, not combined process-tree memory.

## Measurement identity and limits

- Harness source: `d4da91b0a0a5ece52683f28e0915b933bce0d4ad`, merged by
  [PR #3368](https://github.com/Rul1an/assay/pull/3368) as
  `7d39b5996e489a794382e46b6c0b89030e3b7972`.
- Binary: released `v6.14.1` Linux aarch64 `assay-mcp-server`, SHA256
  `787403002c677892593c5bafe89ebe183cb1516e959c5feac1ae72249882960c`.
- Release source: `098ebbeb2efe19c3b8fdc6bc41250c80169cc06c`. Archive digest and
  retained external attestation verification are in `binary-provenance.json` and
  `archive-attestation.json`; operator-supplied metadata is not harness authentication.
- Host: Ubuntu 24.04 VM, Linux `6.8.0-147-generic`, aarch64, four logical CPUs,
  Python 3.12.3, on a shared macOS machine.
- UTC windows: 19:54:10–19:54:12, 19:54:43–19:54:45 and 19:55:16–19:55:18 on
  2026-10-18. Approximately 67 seconds from first start to final completion.
- Exactly three attempts were declared before execution; all completed without
  replacement retries. Thirty-second pauses followed result transfer. No warmup,
  reboot or cache flush; cache state is uncontrolled and may reflect prior work.
- Nine source/dependency hashes and binary hashes matched before/after every run.
  Guest/local result-archive hashes matched. Resource/load observations are snapshots,
  not proof of continuous isolation. The macOS host had other activity.

This series does not establish stability across days, hosts or production workloads,
network/model overhead, tail-latency SLAs, or safe-agent/compliance outcomes.
Raw wire bytes permit call/evidence recounting, but cannot independently reconstruct
clock measurements; retained arm timings and RTT arrays are the timing inputs.

## Retained package and independent recount

The [measurement package](repeatability-2026-10-18.tar.gz) is stored with this
baseline in Git, with its [SHA256 file](repeatability-2026-10-18.sha256).
It contains the original raw bytes, source archive, frozen plan, command/exit
receipts, before/after observations, transfer hashes, comparison and recipient review.
The internal `PACKAGE-MANIFEST.json` covers the original 740 source/evidence files;
seven later report/review files are outside that manifest. The outer archive hash
covers all 747 files. Local paths in historical receipts are provenance, not portable
execution instructions. No release binary is bundled.

From this directory, using Python 3.12 or newer:

```bash
sha256sum -c repeatability-2026-10-18.sha256
# Use a new directory; do not overwrite a previous extraction.
mkdir retained-baseline
tar -xzf repeatability-2026-10-18.tar.gz -C retained-baseline
python3 retained-baseline/repeatability-2026-10-18/independent-review-recount.py
```

The last command reads retained data only; it does not execute the benchmark or
install dependencies. It rewrites its derived `independent-review-recount.json` in
the extraction directory. Keep the original archive as the immutable source.
The recipient independently checked counts, inputs, transfer integrity, medians and
bootstrap intervals, with timing differences no greater than 0.000003 ms from
serialized RTT rounding. See the bundled `independent-review.md` for the exact
review scope and report hash. Claude Code Desktop advised on methodology; a separate
non-building Codex reviewer performed the recipient recount. Agent identities are
declarations, not externally authenticated identities.

## Repeat the measurement

Fetch the pinned release asset and verify its archive/binary digest using the bundled
provenance. Extract `source-d4da91b0a0a5.tar` into a new source directory. From that
source root, invoke the existing command in the [pilot README](README.md), using
`--blocks 20 --calls-per-session 20 --timeout-seconds 30 --bootstrap-resamples 2000`
and a new output directory per attempt. The archived supervisor receipts retain the
original exact argv. Freeze any new attempt budget and environment before execution;
do not rerun the historical `execute.py`, which contains original machine paths.

The next measurement should use a separately declared later time window with these
same pins and workload, retaining all attempts. It must be reported separately from
this series. Time-separated measurements, verifier scaling, other profiles, broader
resource/detection-quality measurements and defensible CI thresholds remain open in
#2494.
