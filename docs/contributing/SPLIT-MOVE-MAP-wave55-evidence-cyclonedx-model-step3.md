# Wave55 Step3 CycloneDX Model Importer Move Map

## Source

`crates/assay-cli/src/cli/commands/evidence/cyclonedx_mlbom_model.rs`

## Destination Modules

| Destination | Moved Responsibility |
| --- | --- |
| `cyclonedx_mlbom_model.rs` | Public `CycloneDxMlBomModelArgs` and `cmd_cyclonedx_mlbom_model` orchestration facade |
| `cyclonedx_mlbom_model/constants.rs` | Event/schema/source/reducer constants and bounded limits |
| `cyclonedx_mlbom_model/events.rs` | Run-id validation, JSON input loading, and `EvidenceEvent` construction |
| `cyclonedx_mlbom_model/reduce.rs` | CycloneDX ML model component selection, dataset/model-card refs, and receipt payload assembly |
| `cyclonedx_mlbom_model/source.rs` | Import-time parsing, default artifact reference, JSON input reading, and source artifact digest |
| `cyclonedx_mlbom_model/validate.rs` | Required/optional bounded reviewer-safe string validation |
| `cyclonedx_mlbom_model/tests.rs` | Existing CycloneDX importer unit tests |

## Preserved Boundaries

- CLI command names and args stay owned by the facade.
- Bundle IO stays in `cmd_cyclonedx_mlbom_model`.
- Event identifiers, schema strings, source labels, and reducer version stay unchanged.
- CycloneDX reduction remains importer-specific; no shared generic importer abstraction is introduced in this step.
- Pydantic and Mastra importers stay untouched.
