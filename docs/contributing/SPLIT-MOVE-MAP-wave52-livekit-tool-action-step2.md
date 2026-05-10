# SPLIT MOVE MAP - Wave 52 LiveKit Tool Action Step2

## Movement Summary

Step2 moves the previous single-file importer into protocol-focused modules
without changing importer behavior.

## LOC Delta

| Path | Before | After |
| --- | ---: | ---: |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs` | 1095 | 81 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/bundle.rs` | 0 | 49 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/canonical.rs` | 0 | 85 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/constants.rs` | 0 | 93 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/input.rs` | 0 | 78 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/reduce.rs` | 0 | 304 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/tests.rs` | 0 | 211 |
| `crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/validate.rs` | 0 | 247 |

## Final Layout

```text
crates/assay-cli/src/cli/commands/evidence/livekit_tool_action.rs
crates/assay-cli/src/cli/commands/evidence/livekit_tool_action/
  bundle.rs
  canonical.rs
  constants.rs
  input.rs
  reduce.rs
  tests.rs
  validate.rs
```

## Region Mapping

- `livekit_tool_action.rs`: CLI args and stable command facade.
- `constants.rs`: event/schema/source strings and key allowlists.
- `input.rs`: input document parsing, import-time parsing, default source refs,
  and source artifact digesting.
- `bundle.rs`: reduced document iteration and `EvidenceEvent` construction.
- `reduce.rs`: receipt reduction, receipt payload construction, and list-order
  call/output pairing.
- `validate.rs`: key validation, bounded reviewer-safe values, booleans, and
  timestamp normalization.
- `canonical.rs`: raw payload canonical JSON hashing and hash/ref selection.
- `tests.rs`: importer unit tests moved unchanged behind the same module.

## Non-Moves

- No CLI route changes.
- No schema file changes.
- No evidence bundle writer changes.
- No Trust Basis classifier changes.
- No public family-matrix update.
