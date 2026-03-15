# ADR-031: Coverage v1.1 DX Polish

## Status
Accepted (March 2026; implemented on `main` via PRs #585, #587, and #588)

## Context
Coverage v1 is schema-stable and already supports:
- JSON report generation via `assay coverage --input ... --out ...`
- optional markdown viewing via `--format md` (v1.0 DX)

Teams want a more PR-friendly dual artifact pattern:
- canonical JSON for machines
- stable markdown written to a file for reviewers and PR attachments

We also want deterministic reviewer signal on route summaries without truncating the canonical JSON.

## Decision
We introduce Coverage v1.1 DX polish (no schema bump):
1) `--out-md <path>`: write a markdown report alongside the canonical JSON report.
2) `--routes-top <N>`: control how many top routes appear in markdown (JSON remains complete).

This is a CLI/DX-only change:
- no workflow changes
- no coverage schema changes
- no MCP wrap behavior changes

## Contract
### Flags
- `assay coverage --input <jsonl> --out <coverage.json> [--out-md <coverage.md>] [--routes-top <N>]`
- `--routes-top` default: `10`

### Output
- JSON output remains `coverage_report_v1` and is the canonical artifact.
- Markdown is derived output and may change formatting without schema bump.

### Exit codes (generator mode)
- `0` success
- `2` measurement/contract (invalid input, schema validation failure)
- `3` infra (write failure for json or md)

## Non-goals
- no schema bump (`coverage_report_v1` unchanged)
- no nightly/workflow wiring
- no MCP wrap changes in this v1.1 line

## Consequences
Positive:
- reviewers get stable file artifacts without losing machine-readable JSON
- route-summary tuning without truncating canonical evidence

Negative:
- another output path to maintain (mitigated by strict tests in B-slice)
