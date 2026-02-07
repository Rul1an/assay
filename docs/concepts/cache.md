# Cache & Incremental Execution

Assay caches run state in SQLite and can skip unchanged passing tests.

---

## Overview

Caching is controlled through CLI flags on `assay run` and `assay ci`:

- `--incremental` skips previously passing tests when fingerprints match.
- `--refresh-cache` forces re-execution (ignores cached state).
- `--no-cache` alias for `--refresh-cache`.
- `--db <PATH>` chooses the SQLite store location (default: `.eval/eval.db`).

There is currently no dedicated `assay cache ...` subcommand.

---

## Common Usage

```bash
# Fast local loop: reuse cache + skip unchanged passing tests
assay run --config eval.yaml --trace-file traces/golden.jsonl --incremental

# Force fresh execution once
assay run --config eval.yaml --trace-file traces/golden.jsonl --refresh-cache

# CI with isolated in-memory DB
assay ci --config eval.yaml --trace-file traces/golden.jsonl --db :memory:
```

---

## CI Guidance

Use `--db :memory:` in CI for deterministic, stateless runs:

```yaml
- name: Assay CI run
  run: |
    assay ci \
      --config eval.yaml \
      --trace-file traces/golden.jsonl \
      --strict \
      --db :memory: \
      --junit .assay/reports/junit.xml \
      --sarif .assay/reports/sarif.json
```

---

## Troubleshooting

### Tests are unexpectedly skipped

```bash
assay run --config eval.yaml --trace-file traces/golden.jsonl --refresh-cache
```

### Different behavior between local and CI

- Ensure the same `eval.yaml` and trace files are used.
- Prefer `--db :memory:` in CI to avoid persisted state.
- Pin the same Assay CLI version across environments.

---

## See Also

- [Replay Engine](replay.md)
- [Traces](traces.md)
- [CLI: assay run](../reference/cli/run.md)
