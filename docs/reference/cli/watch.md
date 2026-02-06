# assay watch

Watch config/policy/trace files and rerun Assay when they change.

---

## Synopsis

```bash
assay watch [OPTIONS]
```

---

## Options

| Option | Description |
|--------|-------------|
| `--config <PATH>` | Config file to watch and run (default: `eval.yaml`). |
| `--trace-file <PATH>` | Trace file used by run loop and watched for changes. |
| `--baseline <PATH>` | Optional baseline file and watch target. |
| `--db <PATH>` | DB path used for runs (default: `.eval/eval.db`). |
| `--strict` | Run in strict mode. |
| `--replay-strict` | Enable strict replay mode in each run. |
| `--clear` | Clear terminal before each rerun. |
| `--debounce-ms <N>` | Debounce window before rerun (default: `350`). |

`assay watch` also resolves and watches policy files referenced by tests in the config.

---

## Examples

```bash
# Watch config + trace and rerun on change
assay watch --config eval.yaml --trace-file traces/dev.jsonl

# Strict loop with terminal clear
assay watch --config eval.yaml --trace-file traces/dev.jsonl --strict --clear
```

---

## Behavior

- Runs once immediately.
- Polls watch targets for changes.
- Debounces bursty edits.
- Re-runs `assay run` with selected flags.
- Stops on `Ctrl+C`.

---

## Exit Codes

`assay watch` is a long-running loop. It exits when interrupted or on unrecoverable startup errors.

---

## See Also

- [assay run](run.md)
- [assay doctor](doctor.md)
