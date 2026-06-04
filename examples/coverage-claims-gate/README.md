# Coverage-claims gate (example consumer)

A small, dependency-free example showing how a downstream gate can **mechanically
enforce** coverage claims instead of merely documenting them.

It reads a coverage annotation sidecar
(`assay.coverage_aware_drift.annotation.v0`, as produced by the cross-runtime
drift comparator's `--coverage-annotation-out`) and a set of asserted claims, then
returns a deterministic pass/fail report:

- every asserted claim permitted → exit `0`
- any claim blocked → exit `1`
- usage / schema error → exit `2`

This is the consumer side of the honesty layer. The annotation already classifies
each drift dimension by claim strength and basis; this checker turns that
classification into an enforceable decision, so a CI step can refuse to assert a
claim the annotation does not support — rather than silently treating absence or
trace-reported signal as if it were measured evidence.

It is an example only. It does not change any Runner or contract surface. The
canonical gate semantics live in `crates/assay-runner-schema/src/coverage.rs`;
this checker mirrors the same claim-kind rules over a frozen annotation document.

## Claim kinds

| Claim spec | Permitted when |
|------------|----------------|
| `positive:DIM` | a `measured_DIM_drift` cell exists with strength `strong` or `partial` |
| `exhaustive:DIM` | an `exhaustive_DIM_equality` cell is allowed (strength `partial`); a coverage-degraded `weak` cell is not |
| `bounded_negative:DIM` | `DIM` is a measured dimension **and** is not present in `blocked_claims`; on a reported/unknown dimension the claim is not evaluable, so not permitted |

Measured dimensions: `filesystem_paths_touched`, `kernel_file_operations`,
`network_endpoints`, `process_execs`. Reported dimensions (e.g. `tool_calls`)
carry no measured ceiling, so bounded-negative claims over them are never
evaluable.

## Usage

```bash
# Inline claims
python3 check_claims.py fixtures/annotation.json \
    --assert-claim positive:filesystem_paths_touched

# Claims from a policy file (JSON array of TYPE:DIMENSION strings)
python3 check_claims.py fixtures/annotation.json --policy fixtures/policy_pass.json

# JSON report instead of text
python3 check_claims.py fixtures/annotation.json \
    --policy fixtures/policy_blocked.json --format json
```

`--assert-claim` and `--policy` combine; at least one claim must be supplied.

## Fixtures

- `fixtures/annotation.json` — a sample annotation with a partial measured
  filesystem positive, a coverage-degraded (`weak`) exhaustive cell, an `absent`
  network positive, a reported `tool_calls` cell, and one blocked bounded-negative
  claim.
- `fixtures/policy_pass.json` — a policy that the annotation permits (exit `0`).
- `fixtures/policy_blocked.json` — a policy mixing permitted and blocked claims
  (exit `1`).
- `fixtures/expected_pass.json`, `fixtures/expected_blocked.json` — the exact JSON
  reports for those two runs, used by the tests.

## Tests

```bash
python3 -m unittest discover -s examples/coverage-claims-gate -p 'test_*.py'
```

Stdlib only — no third-party dependencies.
