# assay replay

Replay a run from a replay bundle.

---

## Synopsis

```bash
assay replay --bundle <BUNDLE.tar.gz> [--live] [--seed <U64>]
```

---

## Description

`assay replay` replays a run using a replay bundle as source of truth.

Default behavior is **offline**:
- no outbound provider calls
- incomplete replay coverage results in `E_REPLAY_MISSING_DEPENDENCY` (exit code `2`)

Use `--live` to allow non-strict replay mode.

Replay verifies the bundle first and then reads only its verified members:
- **Config:** `files/eval.yaml`, else `files/assay.yaml`, else the first YAML or JSON member under `files/` in name order.
- **Trace:** the declared `trace_path`; without one, a trace member chosen by name.
- **Config dependencies:** `sequence_valid.policy`, `args_valid.policy` and `json_schema.schema_file` in the bundled config are relative to the config's directory in the bundle and must name a member the bundle lists, for example `seq.yaml` next to `files/eval.yaml`. Any other reference (absolute, `..`, a backslash, or a name the bundle does not list) is refused before it is read. An ordinary `assay run` keeps its own path rules.

Replay writes `run.json` and `summary.json` in the current working directory and annotates replay provenance:
- `provenance.replay = true`
- `provenance.bundle_digest`
- `provenance.replay_mode`
- `provenance.source_run_id` (when available)

---

## Options

| Option | Description |
|--------|-------------|
| `--bundle <PATH>` | Path to replay bundle archive (`.tar.gz`). |
| `--live` | Enable live replay mode (non-strict). |
| `--seed <U64>` | Override config `settings.seed` before replay run. |

---

## Exit behavior

| Condition | Exit code | reason_code |
|----------|-----------|-------------|
| Replay completed successfully | `0` | `""` |
| Replay bundle invalid / verify failed | `2` | `E_CFG_PARSE` or verify error path |
| Bundled config references a file outside the bundle | `2` | `E_CFG_PARSE` |
| Offline replay missing required dependency | `2` | `E_REPLAY_MISSING_DEPENDENCY` |
| Replay run fails tests | `1` | Run-derived reason code |

---

## Examples

```bash
# Offline replay (default)
assay replay --bundle .assay/bundles/12345.tar.gz

# Live mode
assay replay --bundle .assay/bundles/12345.tar.gz --live

# Override seed
assay replay --bundle .assay/bundles/12345.tar.gz --seed 42
```
