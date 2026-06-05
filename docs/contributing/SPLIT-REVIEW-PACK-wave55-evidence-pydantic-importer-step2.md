# Wave55 Step2 Pydantic Importer Review Pack

## Summary

Step2 splits the Pydantic Evals case-result importer into private modules while keeping the existing
CLI facade and behavior intact.

## Reviewer Focus

- Confirm `pydantic_case_result.rs` only owns CLI args and command orchestration.
- Confirm event/schema constants are unchanged.
- Confirm JSONL reading, event sequencing, payload reduction, source helpers, validation, and tests moved without behavior changes.
- Confirm no schema JSON, Trust Basis, workflow, CycloneDX, or Mastra files changed.

## Proof Snippets

Facade thinness:

```bash
wc -l crates/assay-cli/src/cli/commands/evidence/pydantic_case_result.rs
rg -n '^fn read_case_results|^fn reduce_case_result|^fn validate_top_level|^fn parse_import_time|^fn sha256_file|^const EVENT_TYPE' crates/assay-cli/src/cli/commands/evidence/pydantic_case_result.rs
```

Boundary containment:

```bash
rg -n '^pub\(super\) fn read_case_results|^pub\(super\) fn reduce_case_result|^pub\(super\) fn parse_import_time|^pub\(super\) fn validate_top_level' crates/assay-cli/src/cli/commands/evidence/pydantic_case_result
```

Scope gate:

```bash
BASE_REF=origin/main bash scripts/ci/review-wave55-evidence-pydantic-importer-step2.sh
```

## Expected LOC Delta

| File | Before | After |
| --- | ---: | ---: |
| `pydantic_case_result.rs` | 618 | <= 110 |
| `pydantic_case_result/constants.rs` | 0 | <= 40 |
| `pydantic_case_result/events.rs` | 0 | <= 90 |
| `pydantic_case_result/reduce.rs` | 0 | <= 190 |
| `pydantic_case_result/source.rs` | 0 | <= 60 |
| `pydantic_case_result/validate.rs` | 0 | <= 150 |
| `pydantic_case_result/tests.rs` | 0 | <= 220 |
