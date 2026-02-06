# assay run

Execute a test suite against traces and write run artifacts.

---

## Synopsis

```bash
assay run [OPTIONS]
```

---

## Common Options

| Option | Description |
|--------|-------------|
| `--config <PATH>` | Config file (default: `eval.yaml`) |
| `--db <PATH>` | SQLite DB path (default: `.eval/eval.db`) |
| `--trace-file <PATH>` | Trace file source for replay/validation |
| `--strict` | Treat blocking results as failing exit status |
| `--replay-strict` | Enforce strict replay semantics from trace input |
| `--baseline <PATH>` | Compare against existing baseline |
| `--export-baseline <PATH>` | Export baseline from current run |
| `--no-cache` | Disable cache usage for this run |
| `--refresh-cache` | Ignore incremental cache and re-run |
| `--incremental` | Skip passing tests with unchanged fingerprints |
| `--rerun-failures <N>` | Retry failed tests up to N times |
| `--exit-codes <v1\|v2>` | Exit-code compatibility mode (default: `v2`) |

Judge-related options are available via `--judge`, `--judge-model`, `--judge-samples`, etc.

---

## Examples

```bash
# Basic run
assay run --config eval.yaml --trace-file traces/golden.jsonl

# Strict CI-style run
assay run --config eval.yaml --trace-file traces/golden.jsonl --strict --db :memory:

# Baseline check
assay run --config eval.yaml --trace-file traces/golden.jsonl --baseline assay-baseline.json

# Export baseline
assay run --config eval.yaml --trace-file traces/golden.jsonl --export-baseline assay-baseline.json
```

For dedicated CI report files (SARIF/JUnit/PR comment), use `assay ci`:

```bash
assay ci \
  --config eval.yaml \
  --trace-file traces/golden.jsonl \
  --sarif .assay/reports/sarif.json \
  --junit .assay/reports/junit.xml
```

---

## Outputs

`assay run` writes:
- `run.json` (exit/status/reason metadata)
- `summary.json` (machine-readable summary including seeds and optional judge metrics)
- Console summary + footer

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Test failure / policy failure |
| `2` | Configuration or input error |
| `3` | Infrastructure/judge/provider error |
| `4` | Would block (sandbox/policy) |

For automation, branch on `reason_code` + `reason_code_version` in `run.json` / `summary.json`.

---

## See Also

- [CI Integration](../../getting-started/ci-integration.md)
- [assay import](import.md)
- [assay replay](replay.md)
- [Configuration Reference](../config/index.md)
- [Troubleshooting](../../guides/troubleshooting.md)
