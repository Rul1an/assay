# Runner Fidelity Verdict v0

> **Status:** internal derived helper contract. This page defines the
> `assay.runner.fidelity_verdict.v0` vocabulary and claim gate derived
> from `assay.runner.observation_health.v0`. It does not wire the verdict
> into Runner archives, CLI output, Trust Basis, or any stable public
> report schema.

## Purpose

Runner observation health already records the raw measurement-health
signals:

- `kernel_layer`
- `ringbuf_drops`
- `policy_layer`
- `sdk_layer`
- `cgroup_correlation`

Those fields are the source of truth. The fidelity verdict is a small
derived read-model that answers one narrower question:

```text
Which Runner measured-effect claims may downstream projection/report code
interpret from this observation health record?
```

It is a gate before interpretation, not a new capture layer.

## Schema String

```text
assay.runner.fidelity_verdict.v0
```

The current Rust helper lives in `assay-runner-schema`. The schema string
is an internal helper/contract label at this stage; no JSON Schema
sidecar or archive member is frozen yet.

## Vocabulary Boundary

Do not confuse this verdict with the experiment-scoped observability
calibration `agreement` vocabulary.

| Surface | Scope | Vocabulary | Meaning |
|---|---|---|---|
| Observability calibration | Requested signal vs observed retained signal | `match`, `clipped`, `drift`, `failed`, `not_applicable` | Did a requested comparison/retention target match, clip, drift, fail, or fall outside the measurement surface? |
| Runner fidelity verdict | One Runner `observation_health.v0` record | `clean`, `clipped`, `correlation_partial`, `failed`, `not_applicable` | May measured-effect claims be interpreted from this run's measurement health? |

The calibration vocabulary uses `match` and `drift` because it compares a
requested target with an observed artifact. Runner fidelity deliberately
does not use those words: a health record can be clean without proving
the workload content "matched" anything, and drift is a cross-state
comparison, not per-run measurement quality.

The shared words `clipped`, `failed`, and `not_applicable` are allowed
because they describe measurement states in both contexts. Consumers must
still use the schema namespace to decide which semantics apply.

## Verdicts

| Verdict | Meaning |
|---|---|
| `clean` | Kernel measurement is available, no ring-buffer drops are reported, and cgroup correlation is clean. |
| `clipped` | Measurement ran, but known event loss or partial kernel capture blocks absence/bounded-negative claims. |
| `correlation_partial` | Measurement exists, but the binding between observed effects and the run/cgroup/tool boundary is incomplete. |
| `failed` | The health record is invalid for measured Runner claims, or cgroup correlation failed. |
| `not_applicable` | The platform or layer does not provide the measured kernel-effect surface. Reported claims may still exist, but they are not measured kernel-effect claims. |

## Claim Gate

The helper emits a `claim_gate` with explicit decisions for every
verdict:

| Verdict | `reported_claims` | `measured_positive_claims` | `bounded_negative_claims` | `per_binding_claims` |
|---|---|---|---|---|
| `clean` | `allowed` | `allowed` | `allowed` | `allowed` |
| `clipped` | `allowed` | `degraded` | `blocked` | `allowed` |
| `correlation_partial` | `allowed` | `degraded` | `blocked` | `blocked` |
| `not_applicable` | `allowed` | `blocked` | `blocked` | `blocked` |
| `failed` | `blocked` | `blocked` | `blocked` | `blocked` |

`failed` blocks claims authorized by this fidelity verdict. It does not
erase separately validated SDK, trace, or external receipt artifacts; it
means this Runner health record cannot safely authorize those claims.

The load-bearing rule is:

```text
ringbuf_drops > 0 => clipped => bounded_negative_claims = blocked
```

Observed positive events can remain useful under `clipped`; missing
events cannot prove absence. This is why `clipped` degrades positive
measured claims but blocks bounded negative claims.

## Composition With Projection `claim_level`

Projection helpers such as path projection carry `claim_level` values
like:

- `raw_observed`
- `projected_equivalent`
- `inconclusive`

The fidelity verdict does not replace that vocabulary. It gates which
projection claim levels may be interpreted as measured-effect claims.

Initial composition rule:

| Projection `claim_level` | Fidelity gate consulted |
|---|---|
| `raw_observed` | `claim_gate.measured_positive_claims` |
| `projected_equivalent` | `claim_gate.measured_positive_claims` |
| `inconclusive` | Allowed to remain inconclusive unless the verdict is `failed` |
| unknown value | Blocked |

If a projection or report wants to make a bounded-negative claim, it must
also consult `claim_gate.bounded_negative_claims`. If it wants to bind a
claim to a specific run/cgroup/tool identity, it must also consult
`claim_gate.per_binding_claims`.

This keeps `claim_gate` as a guardrail over existing claim levels instead
of introducing a second independent claim hierarchy.

## Derived Shape

Example clean verdict:

```json
{
  "schema": "assay.runner.fidelity_verdict.v0",
  "source_schema": "assay.runner.observation_health.v0",
  "run_id": "run-001",
  "verdict": "clean",
  "claim_gate": {
    "reported_claims": "allowed",
    "measured_positive_claims": "allowed",
    "bounded_negative_claims": "allowed",
    "per_binding_claims": "allowed"
  },
  "reasons": [
    {
      "field": "observation_health",
      "observed": "clean",
      "rule": "complete_kernel_layer_zero_drops_clean_cgroup_correlation"
    }
  ],
  "non_claims": [
    "fidelity_no_observation_health_replacement",
    "fidelity_no_policy_correctness_verdict",
    "fidelity_no_runtime_safety_verdict",
    "fidelity_no_agent_quality_score",
    "fidelity_no_probabilistic_confidence_score"
  ]
}
```

## Classification Rules

The v0 helper derives the verdict only from one
`ObservationHealth` value:

1. Invalid observation-health records or `cgroup_correlation = failed`
   produce `failed`.
2. Non-Linux platforms or absent kernel layers produce
   `not_applicable`.
3. `ringbuf_drops > 0` or `kernel_layer = partial_ringbuf_drops`
   produces `clipped`.
4. `cgroup_correlation = partial` produces `correlation_partial`.
5. `kernel_layer = complete`, `ringbuf_drops = 0`, and
   `cgroup_correlation = clean` produces `clean`.

If more than one degradation is present, the helper preserves specific
reasons and applies the stricter gate where needed. For example,
partial cgroup correlation blocks per-binding claims even when the
top-level verdict is driven by clipping.

## Non-Claims

- The verdict does not replace `observation_health.v0`.
- The verdict does not prove policy correctness.
- The verdict does not prove runtime safety.
- The verdict does not score agent quality.
- The verdict does not provide probabilistic confidence.
- The verdict does not validate reported traces, spans, tool calls, or
  SDK events as true.

## Wiring Boundary

This slice adds only the contract and helper. It intentionally does not:

- add a Runner archive member;
- add CLI output;
- add Trust Basis claims;
- add capability-diff gating;
- add cross-runtime report wiring;
- add path/network projection Slice 2 or Slice 3.

Report wiring should wait for a concrete consumer or review surface that
needs the derived verdict.
