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
- Preserving doctor exit semantics in dry-run mode: diagnostics that remain exit with the class
  `doctor` returns for them without `--fix`, not with a class of their own.

After apply, doctor re-runs diagnostics and reports remaining error count.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No error-severity diagnostic remains (or fixes resolved them). The same on both channels and with or without `--fix`: the class comes from one function for all three. It also covers the run that checked no config at all, so read `config_check.status` before reading `data_diagnostics[]`; see the limits below. |
| `1` | An error-severity diagnostic remains whose registered class is a test failure, a fix failed to apply, or unsupported fix-mode usage (`--fix` under `--format json`, or `--yes`/`--dry-run` without `--fix`). Also returned when `--fix` is given for a config that will not load ([#2209](https://github.com/Rul1an/assay/issues/2209)). |
| `2` | An explicit `--config` will not load — absent or unreadable alike — and `--fix` was not given, or an error-severity diagnostic remains whose registered class is a config fault. Same class `assay validate` and `assay run` return for the same input. |

Two limits of this table are worth stating rather than leaving a reader to discover them:

- `--fix` and no `--fix` disagree on the class for the same unloadable config, `1` against `2`. The
  `--fix` returns report the outcome of a repair attempt rather than classifying the config, so
  reclassifying them is a decision about what `--fix` means; [#2209](https://github.com/Rul1an/assay/issues/2209) owns it.
  This is the config-load path only; a config that loads and carries an error-severity diagnostic
  gets the same class with and without `--fix`.
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
