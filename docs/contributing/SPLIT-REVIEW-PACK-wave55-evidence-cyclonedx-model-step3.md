# Wave55 Step3 CycloneDX Model Importer Review Pack

## Summary

Step3 splits the CycloneDX ML-BOM model component importer into private modules while keeping the
existing CLI facade and behavior intact.

## Reviewer Focus

- Confirm `cyclonedx_mlbom_model.rs` only owns CLI args and command orchestration.
- Confirm event/schema constants are unchanged.
- Confirm CycloneDX JSON loading, event construction, payload reduction, source helpers, validation, and tests moved without behavior changes.
- Confirm no schema JSON, schema registry, Trust Basis, workflow, Pydantic, or Mastra files changed.

## Proof Snippets

Facade thinness:

```bash
wc -l crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model.rs
rg -n '^fn read_cyclonedx_model_event|^fn reduce_model_component|^fn select_component|^fn parse_import_time|^fn sha256_file|^const EVENT_TYPE' crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model.rs
```

Boundary containment:

```bash
rg -n '^pub\(super\) fn read_cyclonedx_model_event|^pub\(super\) fn reduce_model_component|^pub\(super\) fn parse_import_time|^pub\(super\) fn bounded_string' crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model
```

Scope gate:

```bash
BASE_REF=origin/main bash scripts/ci/review-wave55-evidence-cyclonedx-model-step3.sh
```

## Expected LOC Delta

| File | Before | After |
| --- | ---: | ---: |
| `cyclonedx_mlbom_model.rs` | 607 | <= 100 |
| `cyclonedx_mlbom_model/constants.rs` | 0 | <= 40 |
| `cyclonedx_mlbom_model/events.rs` | 0 | <= 60 |
| `cyclonedx_mlbom_model/reduce.rs` | 0 | <= 220 |
| `cyclonedx_mlbom_model/source.rs` | 0 | <= 60 |
| `cyclonedx_mlbom_model/validate.rs` | 0 | <= 80 |
| `cyclonedx_mlbom_model/tests.rs` | 0 | <= 240 |
