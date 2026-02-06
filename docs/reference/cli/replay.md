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
