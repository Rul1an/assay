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

The diff compares Trust Basis claim levels only. It does not parse Promptfoo
JSONL, inspect external eval payloads, or infer model correctness.

Claim levels are ordered as:

```text
absent < inferred < self_reported < verified
```

Lowering a claim level, or removing a baseline claim, is a regression.
Improving a level, adding a claim, or changing claim metadata is reported but
does not fail unless a future caller adds a stricter policy above this command.

---

## See Also

- [Evidence imports](./evidence.md)
- [Evidence Contract v1](../../spec/EVIDENCE-CONTRACT-v1.md)
- [P34 Trust Basis diff gate plan](../../architecture/PLAN-P34-TRUST-BASIS-DIFF-GATE-2026q2.md)
