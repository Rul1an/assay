# Coverage Command Step2 Move Map

## Target layout

- `crates/assay-cli/src/cli/commands/coverage/mod.rs`
- `crates/assay-cli/src/cli/commands/coverage/generate.rs`
- `crates/assay-cli/src/cli/commands/coverage/legacy.rs`
- `crates/assay-cli/src/cli/commands/coverage/io.rs`
- existing helper modules unchanged in place:
  - `coverage/format_md.rs`
  - `coverage/report.rs`
  - `coverage/schema.rs`

## Mechanical mapping

| Previous location (`coverage.rs`) | New location |
|---|---|
| `cmd_coverage` dispatch | `coverage/mod.rs` |
| `write_generated_coverage_report*` wrappers | `coverage/mod.rs` facade wrappers + `coverage/generate.rs` impl |
| generator-mode argument validation/orchestration | `coverage/generate.rs` |
| declared-tools merge + `--declared-tools-file` load | `coverage/generate.rs` |
| coverage report build + schema validation (`--input` path) | `coverage/generate.rs` |
| legacy analyzer flow + baseline compare/export | `coverage/legacy.rs` |
| file-write helpers + parent dir prep + write logging | `coverage/io.rs` |
| markdown renderer impl | unchanged (`coverage/format_md.rs`) |
| report builder impl | unchanged (`coverage/report.rs`) |
| schema validation impl | unchanged (`coverage/schema.rs`) |

## Invariants preserved

- CLI behavior for `assay coverage` modes is unchanged.
- Exit-code mapping is unchanged.
- JSON output remains canonical artifact; markdown remains derived artifact.
- `--routes-top` behavior remains scoped to markdown rendering path.
- No `assay mcp wrap --coverage-out` path changes in this slice.
