# Wave55 Mastra ScoreEvent Move Map

Baseline:
- Source hotspot: `crates/assay-cli/src/cli/commands/evidence/mastra_score_event.rs`
- Baseline LOC: 557

Resulting layout:

| Target | Responsibility | Notes |
| --- | --- | --- |
| `mastra_score_event.rs` | Stable command facade: module declarations, clap args, `cmd_mastra_score_event` bundle-writing orchestration | Public command surface remains here |
| `mastra_score_event/constants.rs` | Event/source/schema/reducer constants and bounds | `DEFAULT_RUN_ID` remains the clap default source |
| `mastra_score_event/events.rs` | JSONL reader and `EvidenceEvent` construction | Keeps event id sequencing and empty-input behavior unchanged |
| `mastra_score_event/reduce.rs` | Row-to-receipt payload reduction | Keeps payload shape and optional field projection unchanged |
| `mastra_score_event/source.rs` | Import time parsing, default source artifact ref, source file digest | Keeps provenance helpers unchanged |
| `mastra_score_event/validate.rs` | Top-level field validation, bounded strings, timestamp normalization | Keeps rejection messages and reviewer-safe boundaries unchanged |
| `mastra_score_event/tests.rs` | Existing Mastra importer unit tests | Moved out of the facade without changing assertions |

Non-moves:
- `crates/assay-cli/src/cli/commands/evidence/mod.rs` stays unchanged.
- Receipt/input schema JSON files stay unchanged.
- Trust Basis and receipt-family matrix stay unchanged; Mastra remains importer-only.
- No runner/eBPF files are touched.
