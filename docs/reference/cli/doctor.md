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
- Creating a missing trace file for a trace-path error. On doctor's own diagnostics this is the only
  repair reachable today. The patch arms of the shared suggestion builder key on codes that
  `assay_core::validate` and `assay_core::doctor` never emit, and the one code they do share,
  `E_PATH_NOT_FOUND`, needs a `file`/`field` context that neither attaches — so `--fix` prints
  `No auto-fixable diagnostics found` for everything else. `assay fix` is where that machinery has
  reachable inputs.
- Renaming a misspelled config key when the config will not parse, if one expected key is a close
  enough match, and previewing that rename as a unified diff under `--dry-run`.
- Preserving doctor exit semantics in dry-run mode: diagnostics that remain exit with the class
  `doctor` returns for them without `--fix`, not with a class of their own.
- Reporting a repair that could not be written as a config fault — the same class `assay fix`
  returns for a patch it could not apply — with the specific failure on stderr.
- Re-loading the config after writing repairs, and reporting one that no longer loads as a config
  fault rather than as the outcome of the repair.

After apply, doctor re-runs diagnostics and reports remaining error count.

---

## Exit Codes

Every row was measured on this version, one tree per condition. Where the code has a route that no
invocation reaches, the row says so rather than describing it as behaviour.

| Code | Meaning |
|------|---------|
| `0` | No error-severity diagnostic remains, or the fixes resolved them. Measured identical on the text channel, under `--format json` and under `--fix --yes`, because all three read the class from one function. It also covers the run that checked no config at all, so read `config_check.status` before `data_diagnostics[]`; see below. |
| `1` | An error-severity diagnostic whose registered class is a test failure. No input reaching this route has been measured; see below. |
| `2` | Configuration or invocation failure. Argument misuse — `--fix` under `--format json`, or `--yes`/`--dry-run` without `--fix` — exits the invalid-args class. The JSON request publishes one `assay.doctor_report.v0` with `E_INVALID_ARGS`; the text request keeps the existing rejection on stderr and empty stdout. An explicit `--config` that will not load exits this class with or without `--fix`, including when no repair is available, a repair is declined or previewed, or a repair leaves the config unloadable. Error-severity diagnostics registered as config faults and repairs that cannot be written also exit this class. |

Four things the table does not say, which a reader would otherwise have to find out:

- **No `1` has been observed here.** Its route to `1` is an error-severity
  diagnostic whose registered class is not a config fault, and every code doctor names deliberately
  is registered a config fault. That leaves the bare `E_UNKNOWN` raised as a fallback for a trace
  error the mapper does not recognise, and no input reaching it has been constructed. Argument misuse
  and unloadable configs use the classes documented on `2`.
- **A repair request does not reclassify an unloadable config.** `doctor --config nope.yaml` and the
  same config under `--fix --yes` both exit `2`, whether the repair is unavailable, declined,
  previewed, or insufficient. The repair flag changes what doctor attempts, not what condition it
  observed.
- **A failed repair and the diagnostics behind it cannot disagree today.** A repair is offered only
  for `E_PATH_NOT_FOUND` or `E_TRACE_MISS`, both error-severity and both a config fault, so any tree
  where a write can fail already exits `2`. The two are still decided by separate functions, and
  `decide_repair_failure_exit`'s doc comment gives the reason: they answer different questions, and
  coupling them would let a change to the class table redefine what a failed write reports. That
  separation is a guard against a future change, not a difference this version can show — a tree
  whose findings are only advisory is offered no repair at all, so none can fail.
- **Exit `0` also covers the run that checked no config at all.** With no `--config` and no
  `eval.yaml` in the invocation directory, the text path prints `Policy Check: SKIPPED` and the JSON
  report carries `config_check.status: "skipped"` and no `data_diagnostics` key. `checked` is the
  only status under which an empty or absent diagnostics list describes a clean config; `failed` is
  the exit-`2` row above.

---

## See Also

- [assay validate](validate.md)
- [assay watch](watch.md)
- [Troubleshooting](../../guides/troubleshooting.md)
