# Wave55 Step3 CycloneDX Model Importer Split Checklist

## Scope Lock

- [x] Keep `CycloneDxMlBomModelArgs` and `cmd_cyclonedx_mlbom_model` as the public CLI facade.
- [x] Move constants, event construction, payload reduction, source helpers, validation helpers, and unit tests into private submodules.
- [x] Do not modify receipt JSON Schemas, schema registry metadata, Trust Basis mappings, workflows, Pydantic, or Mastra importers.

## Behavior Freeze

- [x] Preserve event type, source, receipt schema, source system, source surface, reducer version, default run id, and bounded-field limits.
- [x] Preserve output bundle writing, producer metadata, import-time parsing, default artifact reference, source artifact digest behavior, and run-id validation.
- [x] Preserve CycloneDX selection/rejection behavior for missing model components, multiple model components, and unmatched `--bom-ref`.
- [x] Preserve inventory-only reduction: keep model component identity and reviewer-safe references without importing model-card bodies, vulnerabilities, licenses, or pedigree.

## Validation

- [x] Keep existing CycloneDX importer unit tests.
- [x] Keep evidence importer integration coverage.
- [x] Keep receipt/input schema validation coverage.
- [x] Add a Wave55 Step3 reviewer gate with allowlist, boundary markers, LOC caps, cargo checks, targeted tests, clippy, and diff checks.
