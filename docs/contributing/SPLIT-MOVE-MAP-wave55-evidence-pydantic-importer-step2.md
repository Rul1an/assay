# Wave55 Step2 Pydantic Importer Move Map

## Source

`crates/assay-cli/src/cli/commands/evidence/pydantic_case_result.rs`

## Destination Modules

| Destination | Moved Responsibility |
| --- | --- |
| `pydantic_case_result.rs` | Public `PydanticCaseResultArgs` and `cmd_pydantic_case_result` orchestration facade |
| `pydantic_case_result/constants.rs` | Event/schema/source/reducer constants and bounded limits |
| `pydantic_case_result/events.rs` | JSONL row reading, event sequencing, and `EvidenceEvent` construction |
| `pydantic_case_result/reduce.rs` | Case-result payload reduction, reduced result normalization, and receipt payload assembly |
| `pydantic_case_result/source.rs` | Import-time parsing, default artifact reference, and source artifact digest |
| `pydantic_case_result/validate.rs` | Top-level/result key validation, bounded strings, schema/source/surface checks, and timestamp normalization |
| `pydantic_case_result/tests.rs` | Existing Pydantic importer unit tests |

## Preserved Boundaries

- CLI command names and args stay owned by the facade.
- Bundle IO stays in `cmd_pydantic_case_result`.
- Event identifiers and schema strings stay unchanged.
- Reducer validation remains Pydantic-specific; no shared generic importer abstraction is introduced in this step.
- CycloneDX and Mastra importers stay untouched for separate future slices.
