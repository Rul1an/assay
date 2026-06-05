# Wave55 Step2 Pydantic Importer Split Checklist

## Scope Lock

- [x] Keep `PydanticCaseResultArgs` and `cmd_pydantic_case_result` as the public CLI facade.
- [x] Move constants, JSONL event reading, reducer logic, source helpers, validation helpers, and unit tests into private submodules.
- [x] Do not modify receipt JSON Schemas, schema registry metadata, Trust Basis mappings, workflows, or non-Pydantic importers.

## Behavior Freeze

- [x] Preserve event type, source, receipt schema, input schema, source system, source surface, reducer version, default run id, and bounded-field limits.
- [x] Preserve output bundle writing, event sequencing, event time, producer metadata, import-time parsing, and source artifact digest behavior.
- [x] Preserve rejection messages for raw ReportCase fields, invalid assertion/score shapes, null optional fields, and unsupported keys.

## Validation

- [x] Keep existing Pydantic importer unit tests.
- [x] Keep evidence importer integration coverage.
- [x] Keep receipt/input schema validation coverage.
- [x] Add a Wave55 Step2 reviewer gate with allowlist, boundary markers, LOC caps, cargo checks, targeted tests, clippy, and diff checks.
