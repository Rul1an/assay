# assay doctor

Diagnose environment/config/trace issues and optionally apply automated fixes.

---

## Synopsis

```bash
assay doctor [OPTIONS]
```

---

## Common Options

| Option | Description |
|--------|-------------|
| `--config <PATH>` | Config file to inspect (default behavior: use `eval.yaml` when present). |
| `--trace-file <PATH>` | Trace file used for deep diagnostics. |
| `--baseline <PATH>` | Baseline file to inspect. |
| `--db <PATH>` | DB path to inspect. |
| `--replay-strict` | Enable strict replay checks in diagnostics. |
| `--format <text\|json>` | Output format (default: `text`). |
| `--fix` | Enable auto-fix mode for known issues. |
| `--yes` | Apply available fixes without prompt (used with `--fix`). |
| `--dry-run` | Preview fixes without writing files (used with `--fix`). |

Note: `--fix` currently supports text output mode.

---

## Examples

```bash
# Basic doctor run
assay doctor --config eval.yaml --trace-file traces/golden.jsonl

# Diagnose and auto-apply available fixes
assay doctor --config eval.yaml --trace-file traces/main.jsonl --fix --yes

# Preview fixes only
assay doctor --config eval.yaml --trace-file traces/main.jsonl --fix --dry-run --yes
```

---

## Fix Behavior

`assay doctor --fix` currently supports:
- Applying patch suggestions generated from diagnostics.
- Creating a missing trace file for trace-path errors.
- Previewing unified diffs in dry-run mode.

After apply, doctor re-runs diagnostics and reports remaining error count.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No blocking diagnostics (or fixes resolved them). |
| `1` | Diagnostics remain, fix failed, or unsupported fix mode usage. |

---

## See Also

- [assay validate](validate.md)
- [assay watch](watch.md)
- [Troubleshooting](../../TROUBLESHOOTING.md)
