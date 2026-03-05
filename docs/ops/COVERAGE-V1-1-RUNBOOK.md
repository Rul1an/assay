# Coverage v1.1 Runbook

## Intent
Operational guidance for Coverage v1.1 DX polish:
- `--out-md` writes a markdown report alongside canonical JSON output
- `--routes-top` controls how many routes appear in markdown (JSON remains complete)

This is a CLI/DX feature only:
- no schema changes (`coverage_report_v1` remains canonical)
- no workflow changes
- no MCP wrap behavior changes

## Canonical artifact
The canonical machine-readable output remains:
- `coverage_report_v1` JSON (schema-validated)

Markdown is derived output for humans and PR review.

## Quickstart

### Generate JSON + Markdown
```bash
assay coverage \
  --input artifacts/decision.jsonl \
  --out artifacts/coverage_report_v1.json \
  --out-md artifacts/coverage_report_v1.md \
  --declared-tools-file declared_tools.txt \
  --routes-top 10
```

## Declared tools file format
- one tool name per line
- blank lines ignored
- lines starting with `#` are comments
- file entries union with any `--declared-tool` flags

Example:

```text
# allowed tools
read_document
web_search
```

## Route summary behavior
- `--routes-top N` affects markdown only
- JSON output remains complete (no truncation)
- `--routes-top 0` hides the route table in markdown

## Exit codes (generator mode)
- `0`: success
- `2`: measurement/contract issues (invalid jsonl, missing required fields, schema validation failure)
- `3`: infra issues (failed to write json or markdown output)

## Troubleshooting

Markdown not written:
- confirm `--out-md` points to a writable location
- check stderr for `Infra error: failed to write ...`

Unexpected exit 2:
- input jsonl is malformed or missing required fields (`tool` or `tool_name`)
- schema validation failed (file a bug with repro)

## References
- ADR-031 Coverage v1.1 DX Polish
- Schema: `schemas/coverage_report_v1.schema.json`
