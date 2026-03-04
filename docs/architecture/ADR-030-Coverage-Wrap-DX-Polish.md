# ADR-030: Coverage + Wrap DX Polish

## Status
Proposed (March 2026)

## Context
The current MCP governance spine is complete and working on `main`:
- tool taxonomy and class-aware matching
- coverage report contract + generator + wrap emission
- session/state window contract + informational wrap export

B4 focuses on CLI/DX polish only. We need a small, low-risk contract for usability improvements without changing policy semantics or runtime enforcement.

## Decision
Introduce a DX-only polish scope for coverage and wrap exports:

1. `assay coverage` adds format selection:
- `--format json|md`
- `md` provides a human-readable summary for PR/review workflows.

2. `assay coverage` adds declared tools file input:
- `--declared-tools-file <path>`
- File is one tool per line.
- Empty lines and comment lines (`# ...`) are ignored.
- Values are unioned with repeated `--declared-tool` flags.

3. `assay mcp wrap` export logging is consistent:
- `--coverage-out` writes one stderr line in a stable style.
- `--state-window-out` writes one stderr line in the same style.

## Constraints (frozen)
- No schema changes (`coverage_report_v1` and `session_state_window_v1` stay unchanged).
- No workflow or required-check changes.
- No behavior changes to enforcement paths.
- Exit precedence remains unchanged: `wrapped > coverage > state-window`.

## Error Handling (frozen)
- Input/contract parsing failures return exit code `2`.
- Output write failures return exit code `3`.
- Wrapped process exit remains authoritative.

## Out of Scope
- Any policy DSL changes.
- Any state backend implementation.
- Any taint/semantic dataflow features.
- Any runtime behavior changes beyond DX output formatting and file-input ergonomics.

## Acceptance Criteria (B4A)
- ADR-030 documents `--format md` and `--declared-tools-file`.
- ADR-030 documents both `--coverage-out` and `--state-window-out` logging consistency.
- ADR-030 documents exit-precedence invariants.
- Reviewer gate enforces docs-only scope + workflow ban.
