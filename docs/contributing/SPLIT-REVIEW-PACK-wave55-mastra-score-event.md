# Wave55 Mastra ScoreEvent Review Pack

Review intent:
- Reduce the Mastra ScoreEvent importer hotspot from a 557-line mixed facade/helper/test file into a thin facade plus focused private modules.
- Preserve every observable behavior: CLI args, exit code, stderr text, bundle writer behavior, event type/source, payload schema, validation boundaries, and test assertions.

Expected LOC after split:

| File | LOC |
| --- | ---: |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event.rs` | 84 |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event/constants.rs` | 10 |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event/events.rs` | 57 |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event/reduce.rs` | 113 |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event/source.rs` | 39 |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event/validate.rs` | 122 |
| `crates/assay-cli/src/cli/commands/evidence/mastra_score_event/tests.rs` | 167 |

Primary review questions:
- Does the facade still expose only `MastraScoreEventArgs` and `cmd_mastra_score_event`?
- Did any validation string, JSON field name, schema id, event type/source, or stderr message change?
- Are tests moved, not weakened?
- Does the review script reject unrelated scope drift?

Verification:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave55-mastra-score-event.sh
```
