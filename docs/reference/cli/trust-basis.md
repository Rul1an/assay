# assay trust-basis

Generate, check and compare canonical Trust Basis artifacts.

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

Without `--out` the report goes to stdout. Neither mode writes anything else.

### Generate with an inputs record

```bash
assay trust-basis generate evidence.tar.gz --output-dir run-42/
```

`--output-dir DIR` creates `DIR` and writes two files into it:

- `trust-basis.json`, byte for byte the report stdout would have carried;
- `trust-basis.inputs.json`, an `assay.trust-basis.inputs.v0` record of the
  inputs that report was generated from.

The record states the SHA-256 and length of the report, the SHA-256 and length
of the raw bundle bytes generation read, each loaded pack (name, version,
`builtin` or `file`, and the digest of its parsed definition, so YAML comments
and layout do not change it) in load order with duplicates kept, the eight
verification limits in effect, whether pack lint ran and with which result cap,
and the Assay version that reported writing it. It holds no timestamps, paths
or command lines.

`DIR` must not exist. The command refuses an existing directory, a symlink, a
missing parent or a parent that is not a directory with exit `2`, and creates
nothing. It never overwrites, cleans up or retries. The pair is checked with
the same validator `verify-inputs` uses, including its size ceilings, before
`DIR` is created, so a failed generation leaves no directory. A record the
reader would refuse, such as one that exceeds 64 KiB because many packs carry
long escaped metadata, exits `3` and creates nothing. The report is written and synced first and the
record last. Any failure after `DIR` exists exits `3` and leaves what was
written; the write is not atomic and claims no durability beyond the syncs it
requests. Files left complete by a failed sync may still bind, but the process
exit still reports the failure. The command assumes the caller controls the
parent directory.

`--output-dir` cannot be combined with `--out`.

---

## Verify inputs

Check that a directory's report matches its inputs record:

```bash
assay trust-basis verify-inputs run-42/ --bundle evidence.tar.gz
```

The reader opens only `DIR/trust-basis.json` and `DIR/trust-basis.inputs.json`,
plus the bundle when `--bundle` names it. It does not search for files. It reads
each member under a fixed ceiling before parsing:

| Input | Ceiling |
|---|---|
| inputs record | 64 KiB |
| report | 1 MiB |
| JSON nesting, either document | 16 levels |
| packs in the record | 64 |
| pack name, version, reported Assay version | 256 UTF-8 bytes |
| bundle, when given | 100 MiB |

Both documents must be exactly the bytes their renderer produces: no extra
whitespace, no duplicate keys, no unknown fields, no `null` in place of an
omitted field, and only integers from `0` to `2^53 - 1`. Every limit must be
positive. The report may hold any subset of known claims; claim-set
completeness is not checked.

JSON output (`--format json`) uses the schema
`assay.trust-basis.inputs-check.v0`. It has an overall `status`, the digests
it observed, one entry per check (`artifact_set`, `sidecar_contract`,
`report_contract`, `report_binding`, `bundle_binding`), and `not_established`.
Checks after the first failure stay `not_evaluated`. `bundle_binding` is
`not_requested` without `--bundle`.

| Status | Exit | Meaning |
|---|---|---|
| `bound` | `0` | The report's bytes match the record, and the bundle's too when given. |
| `incomplete` | `2` | The directory or a member is missing. |
| `invalid` | `2` | A member is not a regular file, is over its ceiling, or breaks its contract. |
| `mismatch` | `2` | A digest or length differs from the record. |
| `unavailable` | `3` | A member exists but could not be read. |

If the result cannot be written to stdout, the exit is `3` whatever the status.

`bound` is a statement about bytes only. The record is unsigned. A pair written
by hand, or an old pair whose bundle has since changed, binds as long as it is
consistent with itself. The command never establishes
`claim_set_completeness`, `environment_completeness`, `freshness`,
`generation_authenticity` or `pack_execution`, and it lists them in every
result. It says nothing about who generated the pair, whether the claims are
true, or whether the record may be reused for another run.

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

## Assert

Assert required claim levels in one canonical Trust Basis artifact:

```bash
assay trust-basis assert \
  --input trust-basis.json \
  --require external_eval_receipt_boundary_visible=verified
```

`assert` is intentionally smaller than `diff`. It reads one
`trust-basis.json` artifact, keys requirements only by stable `claim.id`, and
checks that each requested claim has the expected level.

Multiple requirements are allowed:

```bash
assay trust-basis assert \
  --input trust-basis.json \
  --require bundle_verified=verified \
  --require external_decision_receipt_boundary_visible=verified
```

Use JSON output for CI consumers:

```bash
assay trust-basis assert \
  --input trust-basis.json \
  --require external_inventory_receipt_boundary_visible=verified \
  --format json
```

JSON output uses the stable machine-readable schema
`assay.trust-basis.assert.v1` and includes:

- `summary`
- `requirements`
- `claim_id`
- `expected_level`
- `actual_level`
- `status`

Missing claims are policy mismatches, not successes. Unknown claim IDs, unknown
levels, malformed requirements, duplicate claim IDs in the input artifact, and
parse errors are input/config failures.

Exit codes are:

- `0` when all requirements are satisfied.
- `1` when at least one requirement does not match.
- Other non-zero codes for input, parse, or validation failures.

`assert` does not compare baseline and candidate artifacts, replace
`trust-basis diff`, or add Promptfoo/OpenFeature/CycloneDX-specific policy. It
is a generic claim-id gate over one Trust Basis artifact.

---

## See Also

- [Evidence imports](./evidence.md)
- [Evidence Contract v1](../../spec/EVIDENCE-CONTRACT-v1.md)
- [Receipt family matrix](../receipt-family-matrix.json)
- [P34 Trust Basis diff gate plan](../../architecture/PLAN-P34-TRUST-BASIS-DIFF-GATE-2026q2.md)
