# Coverage-Aware Drift Annotation (sample)

This sample reads one `assay.runner.runtime_drift.v0.2` report and attaches
per-dimension claim cells that apply the coverage ceiling at the
comparator-output level, using the same gate as the shipped
`assay.runner.coverage_descriptor.v0` helper
(`crates/assay-runner-schema/src/coverage.rs`).

It is the comparator-level companion to the single-archive
[`../coverage-aware-side-effect/`](../coverage-aware-side-effect/README.md)
sample. That sample gates claims for one measured run; this one gates the
*reading* of a cross-runtime drift row.

It is intentionally small and derived-report only:

- reads one frozen `runtime_drift.v0.2` report fixture
- emits an `assay.coverage_aware_drift.annotation.v0` placeholder annotation
- does not change the comparator, the runtime-drift schema, Runner archives,
  or Trust Basis

## What it shows

The drift comparator classifies each dimension as `task-induced`,
`runtime-induced`, `inconclusive`, etc. The hazard is reading a `task-induced`
(full-overlap) filesystem or network row as "these are exhaustively all the
effects, and they match." Coverage descriptors say otherwise: filesystem
capture is `open_syscall_only` and network capture is `connect_only`, so neither
backs an exhaustive-equality or bounded-negative claim. For each measured
dimension the annotation therefore distinguishes:

- positive drift (this surface was observed) is `partial measured`. It is
  capped at partial here on purpose: the drift report does not surface per-arm
  observation health, so this sample will not emit a strong measured claim it
  cannot back from the report alone. Consult `fidelity_verdict.v0` against the
  source archives to raise it.
- exhaustive equality (this is the complete shared effect set) is `weak
  measured` while the dimension declares blind spots.
- bounded negative (no effect beyond the observed surface) is blocked, even for
  an empty or inconclusive row. A zero-observation row is exactly where someone
  is tempted to claim "no effects happened"; the annotation refuses it.

SDK/trace-reported dimensions (`sdk_tool_events`, `tool_invocation_order`,
`mcp_tool_surface`) stay `reported` basis with no coverage gate, because they are
not measured kernel effects.

A `task-induced` classification is also recorded as a caveat, so full overlap is
read as descriptive surface shape, not as exhaustive-equality proof.

## Files

- `annotate_drift.py`: derived-annotation generator (stdlib only)
- `fixtures/drift_report.json`: small frozen `runtime_drift.v0.2` report
- `fixtures/expected_annotation.json`: frozen expected annotation
- `test_annotate.py`: stdlib tests for the partial / weak / blocked outcomes

## Run

```bash
python3 examples/coverage-aware-drift-annotation/annotate_drift.py \
  examples/coverage-aware-drift-annotation/fixtures/drift_report.json
python3 examples/coverage-aware-drift-annotation/annotate_drift.py \
  examples/coverage-aware-drift-annotation/fixtures/drift_report.json --format markdown
python3 examples/coverage-aware-drift-annotation/test_annotate.py
```

## Boundary

This is a sample annotation generator. It classifies the evidence a drift report
already carries; it does not change the comparator, certify a runtime, prove
intent, or claim a complete view of side effects. The canonical gate logic is the
Rust `coverage_descriptor.v0` helper; this script mirrors its ceiling so the
pattern is reviewable from a frozen report. Wiring this annotation into the
comparator itself is a later Runner-crate slice.
