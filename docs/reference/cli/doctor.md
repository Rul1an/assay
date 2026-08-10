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
| `0` | On the text channel: no blocking diagnostics (or fixes resolved them). With `--format json` the code is weaker than that — it also covers a report carrying an error-severity diagnostic, and one where no config was checked at all. Read `config_check.status` and `data_diagnostics[].severity`; see the limits below. |
| `1` | Diagnostics remain, fix failed, or unsupported fix mode usage. Also returned when `--fix` is given for a config that will not load ([#2209](https://github.com/Rul1an/assay/issues/2209)). |
| `2` | An explicit `--config` will not load — absent or unreadable alike — and `--fix` was not given. Same class `assay run` returns for the same file. |

Three limits of this table are worth stating rather than leaving a reader to discover them:

- `--fix` and no `--fix` disagree on the class for the same unloadable config, `1` against `2`. The
  `--fix` returns report the outcome of a repair attempt rather than classifying the config, so
  reclassifying them is a decision about what `--fix` means; [#2209](https://github.com/Rul1an/assay/issues/2209) owns it.
- With `--format json`, exit `0` does **not** mean the report carries no error-severity diagnostic.
  The text path returns `1` in that case and the JSON path returns `0`;
  [#2215](https://github.com/Rul1an/assay/issues/2215) owns the reconciliation. Read
  `data_diagnostics[].severity` rather than the exit code alone.
- Exit `0` also covers the run that checked no config at all. With no `--config` and no `eval.yaml`
  in the invocation directory, the text path prints `Policy Check: SKIPPED` and the JSON report
  carries `config_check.status: "skipped"` and no `data_diagnostics` key. `checked` is the only
  status under which an empty or absent diagnostics list describes a clean config; `failed` is the
  exit-`2` row above.

---

## See Also

- [assay validate](validate.md)
- [assay watch](watch.md)
- [Troubleshooting](../../guides/troubleshooting.md)
