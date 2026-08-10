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

Notes:
- `--fix` currently supports text output mode.
- `--yes` and `--dry-run` require `--fix`.
- `--dry-run` previews fixes but still returns non-zero when blocking diagnostics remain.

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
- Preserving doctor exit semantics in dry-run mode (blocking diagnostics still exit with code `1`).

After apply, doctor re-runs diagnostics and reports remaining error count.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No blocking diagnostics (or fixes resolved them). |
| `1` | Diagnostics remain, fix failed, or unsupported fix mode usage. Also returned when `--fix` is given for a config that will not load ([#2209](https://github.com/Rul1an/assay/issues/2209)). |
| `2` | An explicit `--config` will not load — absent or unreadable alike — and `--fix` was not given. Same class `assay run` returns for the same file. |

Two limits of this table are worth stating rather than leaving a reader to discover them:

- `--fix` and no `--fix` disagree on the class for the same unloadable config, `1` against `2`. The
  `--fix` returns report the outcome of a repair attempt rather than classifying the config, so
  reclassifying them is a decision about what `--fix` means; [#2209](https://github.com/Rul1an/assay/issues/2209) owns it.
- With `--format json`, exit `0` does **not** mean the report carries no error-severity diagnostic.
  The text path returns `1` in that case and the JSON path returns `0`;
  [#2215](https://github.com/Rul1an/assay/issues/2215) owns the reconciliation. Read
  `data_diagnostics[].severity` rather than the exit code alone.

---

## See Also

- [assay validate](validate.md)
- [assay watch](watch.md)
- [Troubleshooting](../../guides/troubleshooting.md)
