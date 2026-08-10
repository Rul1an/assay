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
- Reporting a repair that could not be written as a config fault — the same class `assay fix`
  returns for a patch it could not apply — with the specific failure on stderr.

After apply, doctor re-runs diagnostics and reports remaining error count.

---

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | No error-severity diagnostic remains (or fixes resolved them), and under `--fix` no repair failed to write. Both channels read the diagnostics' class from one function, and both flag paths read that same function. It also covers the run that checked no config at all, so read `config_check.status` before reading `data_diagnostics[]`; see the limits below. |
| `1` | Every `1` doctor emits today comes from a literal that an open issue owns, not from the class table. Argument misuse — `--fix` under `--format json`, or `--yes`/`--dry-run` without `--fix` — returns `1` instead of the invalid-argument reason code that `assay init` publishes for the same category ([#2208](https://github.com/Rul1an/assay/issues/2208)); `--fix` for a config that will not load returns `1` from the parse-repair path ([#2209](https://github.com/Rul1an/assay/issues/2209)). The table's own route to `1` — an error-severity diagnostic whose class is not a config fault — needs a code the class table does not register: every code doctor names deliberately is registered a config fault, so the reachable route is the bare `E_UNKNOWN` an unexpected trace error is reported as. |
| `2` | An explicit `--config` will not load — absent or unreadable alike — and `--fix` was not given; an error-severity diagnostic remains whose registered class is a config fault; or `--fix` could not write a repair. Same class `assay validate` and `assay run` return for the same input, and the same class `assay fix` returns for a patch that fails to apply. |

Two limits of this table are worth stating rather than leaving a reader to discover them:

- One flag-dependence remains, and it is bounded to the config that does not load:
  `doctor --config nope.yaml` exits `2` where `doctor --config nope.yaml --fix --yes` exits `1`.
  Those `--fix` returns report the outcome of a repair attempt rather than classifying the config,
  and [#2209](https://github.com/Rul1an/assay/issues/2209) owns them. Where the config does load,
  the class does not depend on the flag: measured on one tree carrying a single `E_PATH_NOT_FOUND`,
  text, `--format json`, `--fix --dry-run` and `--fix --yes` all exit `2`, including when the repair
  itself fails to write. That is a measurement over those conditions, not a claim about every input.
  One difference is deliberate rather than a defect: a repair that fails to write exits `2` even on a
  tree whose findings are only advisory, where `doctor` alone exits `0`. A write this process
  attempted and could not complete is not a clean run, so that case is reported as a fault rather
  than read off the diagnostics.
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
