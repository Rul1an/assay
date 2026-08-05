# ADR-045 Landlock seal fixture slice

Status: implementation-fixture slice for issue #1998.

This experiment is deliberately narrower than the AEE fixture spike in
`docs/experiments/aee-assay-spike-2026-08.md`. It does not add a production AEE
exporter and does not claim production AEE support. It pins the first checker
contract for the ADR-045 Landlock/TCP-connect run-end seal primitive.

## Files

- `scripts/experiments/aee_landlock_seal_fixture.py` emits and validates named
  ADR-045 Landlock seal fixtures.
- `scripts/experiments/fixtures/aee-landlock-seal/valid-landlock-seal.json`
  is the positive fixture marker.
- `scripts/experiments/fixtures/aee-landlock-seal/negative-controls/*.json`
  are targeted invalid fixture markers.

The committed JSON files are intentionally small markers for the named cases.
Run `--emit` to materialize the full canonical JSON fixture bodies. This keeps
the PR reviewable while preserving deterministic fixture generation from one
checker/emitter implementation.

## Smoke check

```sh
python3 scripts/experiments/aee_landlock_seal_fixture.py --emit
python3 scripts/experiments/aee_landlock_seal_fixture.py valid-landlock-seal
for name in \
  missing-seal \
  bad-run-binding \
  not-still-armed \
  bad-drop-accounting \
  uncounted-channel-without-eligible-seal \
  bad-observed-set \
  unsupported-observed-attack \
  substrate-runner-observed-attacks-mismatch \
  fixture-key-production-scope
 do
  python3 scripts/experiments/aee_landlock_seal_fixture.py "$name" --expect-invalid
 done
```

## Contract pinned by this slice

The checker fails closed when:

- a substrate row has no sealed coverage;
- a seal carries a bad run binding;
- `aeeStillArmed` is not true;
- zero drop accounting is asserted without an eligible proof model;
- `aeeObservedSet` does not recompute over emitted interception/examination record leaves;
- `aeeObservedAttacks` names attacks unsupported by caught rows;
- substrate-runner attribution requires equality and the lower-bound set does not match;
- a fixture key is used in a production-like path.

## Non-claims

This slice does not claim:

- production AEE support;
- stable AEE export;
- production signing semantics;
- complete run-population proof;
- agent safety;
- provider side-effect verification.
