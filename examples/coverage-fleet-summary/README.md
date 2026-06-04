# Coverage fleet summary (example aggregator)

A small, dependency-free example showing how the per-run coverage honesty
classification scales to a whole set of runs — using only local inputs and with
no contract change. It is a deterministic fold over annotation documents on disk.

It reads many coverage annotation sidecars
(`assay.coverage_aware_drift.annotation.v0`, as produced by the cross-runtime
drift comparator's `--coverage-annotation-out`) and emits one fleet-level summary
(`assay.coverage_fleet_summary.v0`, example-scoped, v0): for each measured
dimension it reports the distribution of positive strengths, the distribution of
exhaustive-equality strengths, how many runs block the bounded-negative claim,
and the **fleet floor** — the weakest positive strength seen across the set.

The fleet floor answers the operational question directly: "across these runs,
the strongest positive claim I can make *everywhere* is no better than this." If
one run in the set degraded to `absent`, the floor is `absent`, even if every
other run was `strong`.

It is an example only — it changes no Runner or contract surface. It consumes
only the public annotation sidecar shape.

## Usage

```bash
# A directory of annotation .json files (sorted, non-recursive)
python3 aggregate_coverage.py --dir fixtures/runs

# Explicit files
python3 aggregate_coverage.py fixtures/runs/run-01.json fixtures/runs/run-02.json

# JSON summary
python3 aggregate_coverage.py --dir fixtures/runs --format json
```

## What the summary contains

For each of the measured dimensions (`filesystem_paths_touched`,
`kernel_file_operations`, `network_endpoints`, `process_execs`):

- `measured_positive` — count of runs at each strength (`strong`, `partial`,
  `weak`, `absent`, `missing`).
- `exhaustive_equality` — count of runs at each exhaustive-claim strength.
- `bounded_negative_blocked` — how many runs blocked the absence-beyond-observed
  claim for this dimension.
- `runs_observed` — runs that carried any measured positive cell for the
  dimension.
- `fleet_positive_floor` — the weakest positive strength observed, or `missing`
  if the dimension was never observed across the fleet.

## Fixtures

`fixtures/runs/` holds three synthetic annotation documents with deliberately
varied coverage (a clean run, a clipped run, and a run that failed fidelity on
one arm). `fixtures/expected_summary.json` is the exact fold of those three, used
by the tests.

## Tests

```bash
python3 -m unittest discover -s examples/coverage-fleet-summary -p 'test_*.py'
```

Stdlib only — no third-party dependencies.
