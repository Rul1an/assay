# Coverage-Aware Side-Effect Report (sample)

This sample turns a Runner archive's `observation_health` and
`capability_surface` into per-dimension claim cells plus blocked claims, applying
the same claim-kind gate as the shipped `assay.runner.coverage_descriptor.v0`
helper (`crates/assay-runner-schema/src/coverage.rs`).

It is intentionally small and derived-report only:

- reads one frozen archive fixture (observation_health + capability_surface)
- emits a `assay.coverage_aware_side_effect.report.v0` placeholder report
- does not register a new archive member, change Runner schemas, or promote
  anything into Trust Basis

## What it shows

For each observed effect dimension, the report distinguishes claim kinds:

- positive existence (this open / connect / exec happened) is `strong measured`
  when capture is clean, `partial` when capture is degraded, and `absent` (with
  no evidence refs) on the not_applicable path, where there is no measured
  kernel surface (non-Linux, or kernel layer absent); capture health, not blind
  spots, gates its strength. Out-of-contract health (for example
  `cgroup_correlation=failed`) is rejected outright rather than interpreted
- exhaustive set (these are all the X) is `weak derived` while the dimension's
  coverage declares blind spots: the exhaustive reading is computed by the gate,
  so its basis is `derived`, and the cell note names the rule
- bounded negative (X did not happen) is blocked unless coverage is complete
  with no blind spots and capture was clean

The point is the last one: absence of an observed effect is not proof that the
effect did not occur, so the report refuses that claim rather than letting a zero
read as safety.

## Files

- `report_from_archive.py`: derived-report generator (stdlib only)
- `fixtures/clean.archive.json`: clean capture, filesystem + connect-only network
- `fixtures/clipped.archive.json`: ring-buffer drops, so positive claims degrade
- `fixtures/clean.report.json`: frozen expected report for the clean fixture
- `test_report.py`: stdlib tests for the strong / weak / blocked outcomes

## Run

```bash
python3 examples/coverage-aware-side-effect/report_from_archive.py \
  examples/coverage-aware-side-effect/fixtures/clean.archive.json
python3 examples/coverage-aware-side-effect/report_from_archive.py \
  examples/coverage-aware-side-effect/fixtures/clean.archive.json --format markdown
python3 examples/coverage-aware-side-effect/test_report.py
```

## Boundary

This is a sample report generator. It classifies the evidence a Runner archive
already carries; it does not certify a server, prove intent, or claim a complete
view of side effects. The canonical gate logic is the Rust
`coverage_descriptor.v0` helper; this script mirrors it so the pattern is
reviewable from a frozen fixture.
