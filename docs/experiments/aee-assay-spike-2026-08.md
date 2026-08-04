# Assay -> AEE v0.7 fixture spike

Status: non-production experiment.

This experiment tests whether Assay's current runtime/proxy evidence carriers can be shaped into an Adversarial Execution Evidence (AEE) v0.7 statement for a two-vantage substrate under one operator.

It deliberately builds the seal first. The first useful result is not production AEE support; it is the boundary statement that Assay can synthesize an AEE-shaped fixture seal, while production Assay does not yet emit a substrate-signed post-run seal over still-armed, drop accounting, observed labels, and observed attacks.

## Pinned external input

- AEE PR: `in-toto/attestation#570`
- AEE PR head used for this spike: `c0c4da67defdf0f186f162e7ecb3f9527b6a94f8`
- AEE predicate version: `0.7`
- Local SHA-256 of `spec/predicates/adversarial-execution-evidence.md` from that head: `fda0f5f7885d56feb93194cfa604f57c060c12677f77fa5579888b15dc1d1a2d`

The predicate is still under vetting. Any field or validity rule can change before acceptance.

## Assay surfaces used

The fixture maps two existing Assay evidence surfaces:

1. `assay.denied_call_observation.v0`
   - Used as the source for the proxy interception payload.
   - Preserves the existing boundary: this is a caller-visible observation, not a policy verdict and not proof of upstream side effect.
2. `assay.enforcement_health.v1`
   - Used as the source for the Landlock TCP-connect blocked-probe payload.
   - Preserves the existing boundary: this proves a blocked probe in the fixture, not complete run population.

The fixture also carries synthetic substrate, catch-policy, corpus, network-posture, arming, and sealed records. Those synthetic records are the spike mechanism, not current production Assay output.

## Files

- `scripts/experiments/aee_spike_emit.py` emits the valid statement and optional negative controls.
- `scripts/experiments/aee_spike_check.py` checks the fixture statement invariants.
- `scripts/experiments/fixtures/aee/` contains the Assay-shaped source fixtures. The emitter writes generated AEE-shaped statements into this directory.

## Run

```bash
python3 scripts/experiments/aee_spike_emit.py --variants
python3 scripts/experiments/aee_spike_check.py scripts/experiments/fixtures/aee/statement-valid.json
for f in scripts/experiments/fixtures/aee/negative-controls/*.json; do
  python3 scripts/experiments/aee_spike_check.py --expect-invalid "$f"
done
```

## What the checker validates

This checker is intentionally not a general AEE verifier. It validates the spike properties needed for the Assay mapping:

- exactly one subject;
- corpus manifest digest recomputation;
- observation vocabulary digest recomputation;
- run-binding recomputation using AEE v0.7 binding version 2 inputs;
- fixture signature verification for carried observation records;
- `batchRoot` recomputation over carried records;
- substrate rows carry arming, sealed, and interception coverage;
- every carried covering-kind record is constrained even when no row references it;
- pinned rows match `expectedPayloads` commitments;
- negative controls fail for the expected reason.

## Negative controls

The emitted negative controls cover the current spike risks:

1. `statement-missing-seal.json`
   - Expected failure: substrate rows lack required sealed coverage.
2. `statement-defective-unreferenced-seal.json`
   - Expected failure: a carried but unreferenced covering-kind seal still violates constraints.
3. `statement-artifact-labelled-substrate.json`
   - Expected failure: a substrate row with no observation references cannot be credited.
4. `statement-reconstructed-priced-intercepted.json`
   - Expected failure: a row claiming `intercepted` is covered by a weaker `reconstructed` record.
5. `statement-run-population-overclaim.json`
   - Expected failure: the spike must not claim no sibling runs existed.

## Non-claims

This experiment does not claim:

- production AEE support;
- stable support before the AEE predicate is accepted;
- complete run population;
- agent safety;
- independent substrate operation when one fixture key signs both collection paths;
- that production Assay emits substrate-signed sealed records;
- that ProtoJSON or generated bindings are canonical evidence.

## Initial finding

The fixture supports the expected first finding:

> Assay can synthesize AEE-shaped evidence for a two-vantage run only if it has an AEE sealed record. Current Assay carriers provide attach/probe/count and proxy-observation facts, but not yet a substrate-signed post-run seal over still-armed, drop-count/drop-bound, observed-set, and observed-attacks.

The next useful work is to decide whether that seal belongs as a production Assay primitive, and whether one AEE `substrate` descriptor is sufficient for the proxy plus Landlock two-vantage case without losing important collection-path semantics.
