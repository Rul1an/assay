# assay trust-basis

Generate and compare canonical Trust Basis artifacts.

---

## Synopsis

```bash
assay trust-basis <COMMAND> [OPTIONS]
```

---

## Generate

Generate `trust-basis.json` from a verified evidence bundle:

```bash
assay trust-basis generate evidence.tar.gz --out trust-basis.json
```

`trust-basis.json` is the small claim artifact above an evidence bundle. It
does not re-state raw evidence payloads; it records which bounded trust claims
were visible to the compiler and at what level.

---

## Diff

Compare a baseline Trust Basis artifact with a candidate Trust Basis artifact:

```bash
assay trust-basis diff baseline.trust-basis.json candidate.trust-basis.json
```

Both inputs are canonical Trust Basis JSON files produced by
`assay trust-basis generate`. The diff keys claim comparison by stable
`claim.id`; duplicate claim IDs are rejected as invalid inputs.

Use JSON output for CI and Harness-style consumers:

```bash
assay trust-basis diff \
  baseline.trust-basis.json \
  candidate.trust-basis.json \
  --format json
```

Use `--fail-on-regression` when the comparison should become a gate:

```bash
assay trust-basis diff \
  baseline.trust-basis.json \
  candidate.trust-basis.json \
  --fail-on-regression
```

The diff compares Trust Basis claim presence and levels only. It does not parse
Promptfoo JSONL, CycloneDX BOMs, external receipt payloads, or infer model,
decision, inventory, or upstream-tool correctness.

Claim identity is determined solely by `claim.id`. Source, boundary, and note
differences do not create a different claim identity; they are reported
separately as metadata changes.

Claim levels are ordered as:

```text
absent < inferred < self_reported < verified
```

Lowering a claim level, or removing a baseline claim, is a regression.
Improving a level, adding a claim, or changing claim metadata is reported but
does not fail unless a future caller adds a stricter policy above this command.
New or unknown claim IDs in the candidate are additions, not regressions.

Source, boundary, and note changes are reported as metadata changes. In v1 they
are review-visible and non-blocking; the gate fails only on missing baseline
claims or lowered levels when `--fail-on-regression` is set.

JSON output uses the stable machine-readable schema
`assay.trust-basis.diff.v1` and includes:

- `summary`
- `regressed_claims`
- `improved_claims`
- `removed_claims`
- `added_claims`
- `metadata_changes`
- `unchanged_claim_count`

Diff arrays are sorted deterministically by `claim.id`.

P34/P35/P36 consumers should treat `assay.trust-basis.diff.v1` JSON as the
canonical machine contract and must not infer regressions from ad hoc text
output.

Exit codes are:

- `0` for successful comparisons with no gate failure.
- `1` when `--fail-on-regression` is set and regressions are present.
- Other non-zero codes for input, parse, or validation failures.

---

## See Also

- [Evidence imports](./evidence.md)
- [Evidence Contract v1](../../spec/EVIDENCE-CONTRACT-v1.md)
- [P34 Trust Basis diff gate plan](../../architecture/PLAN-P34-TRUST-BASIS-DIFF-GATE-2026q2.md)
