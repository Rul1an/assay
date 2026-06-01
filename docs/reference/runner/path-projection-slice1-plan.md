# Runner Path Projection — Slice 1 Plan (declared-rule only)

> **Status:** slice-scoping plan, not a shipped contract. This narrows
> the path-projection work already described in
> [`projection-roadmap.md`](projection-roadmap.md) into the smallest
> buildable first slice. It does not change the Runner archive v0
> contracts, add a public CLI surface, infer heuristic classes, touch
> network projection, or promote projection output to primary evidence.

## Why a slice, not a new plan

The projection roadmap already defines the path classes, the network
taxonomy, the confidence levels, and the acceptance criteria. The
cross-runtime drift comparator already carries that taxonomy as
**vocabulary-only** metadata: it validates declared projection classes
and preserves `unknown`, but it does **not yet infer** a class from a raw
path or endpoint.

So the gap is not more planning. It is one bounded implementation step:
turn the documented taxonomy into a computed projection for the
highest-confidence case only. This slice scopes that step.

## Slice 1 scope

Implement path projection for **declared-rule mappings only**:

- workload-contract paths (declared input / output / scratch), and
- the run-workdir prefix when it is declared by workflow or artifact
  metadata.

Everything else stays `unknown`. No heuristics, no network, no taxonomy
inference beyond declared rules.

In scope:

```text
raw filesystem_paths (unchanged, still source of truth)
  + projected rows for declared matches only:
      raw_path, projected_path, path_class, rule, confidence=declared
  + unmatched raw paths summarized by count + samples, class=unknown
```

Explicitly **out of this slice** (later slices, same engine):

- Slice 2: heuristic path classes (`runtime_package`, `provider_sdk`,
  `loader`, `cache`) at `confidence=heuristic`.
- Slice 3: network endpoint projection (`provider_api`, `dns`,
  `telemetry`, `package_fetch`) reusing the same projection engine and
  confidence model.

## Output shape (extends the roadmap example)

```json
{
  "raw_path": "/opt/actions-runner/_work/assay/.../workdir/fixture-input.txt",
  "projected_path": "workdir/input",
  "path_class": "workload_fixture",
  "rule": "workload_contract_input_path",
  "confidence": "declared",
  "relation": "inside_run_workdir"
}
```

The raw `filesystem_paths` set is unchanged. Projected rows are additive.

## Acceptance rules

1. Raw path sets remain present and unchanged; projection is additive.
2. Every projected row carries `rule` and `confidence`.
3. Slice 1 emits `confidence=declared` only. A row that would require a
   prefix/substring heuristic is left `unknown`, not guessed.
4. Workdir-prefix stripping is allowed only when the prefix is declared
   by workflow or artifact metadata; an undeclared prefix stays raw.
5. Unmatched raw paths are summarized by count and samples; `unknown` is
   not a failure.
6. Projection is idempotent: projecting a projected report is a no-op.
7. Projection emits no policy verdict. `path_class` is reviewer
   vocabulary, not an allow/deny or safety judgment.
8. A report can show "raw paths differ, projected paths match" without
   asserting the workloads are semantically equivalent.

## Non-claims

- This slice does not classify a path as safe, malicious, expected, or
  policy-relevant. Class strings are reviewer vocabulary only.
- This slice does not infer heuristic classes; only declared rules
  produce a class in Slice 1.
- This slice does not change the Runner archive v0 schema, add a CLI
  surface, or promote projection to primary evidence.
- A projected match is not a claim of behavioral equivalence between two
  runs; raw rows remain the source of truth.
- `unknown` is an honest absence of a declared rule, not a finding.

## Slice gate (review-ready when)

- Declared-rule path projection emits additive rows with
  `confidence=declared` for the workload-contract and declared-workdir
  cases.
- Raw `filesystem_paths` round-trip unchanged.
- Idempotence and `unknown`-preservation are covered by tests.
- A fixture shows "raw differs / projected matches" with the
  non-equivalence non-claim attached.
