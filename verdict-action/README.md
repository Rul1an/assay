# Verdict Action

Run Verdict evaluations in CI/CD pipelines (GitHub Actions).

Supports:
- **Baseline Gating**: Fail PRs if metrics degrade compared to `main`.
- **Replay**: Deterministic evaluation from trace files.
- **Reporting**: SARIF (Code Scanning), JUnit, and Artifacts.

## Inputs

| Input | Default | Description |
|------|---------|-------------|
| `repo` | `Rul1an/verdict` | GitHub repo hosting Verdict releases |
| `verdict_version` | *(required)* | Release tag (e.g. `v0.3.4`) |
| `config` | *(required)* | Config YAML path (relative to `workdir`) |
| `workdir` | `.` | Working directory (monorepo support) |
| `trace_file` | `""` | Trace JSONL (relative to `workdir`) |
| `baseline` | `""` | Baseline JSON (relative to `workdir`) |
| `export_baseline` | `""` | Export baseline to this path (relative to `workdir`) |
| `sarif` | `auto` | `auto|true|false` — auto skips fork PRs |
| `upload_baseline_artifact` | `true` | Upload exported baseline as an artifact |
| `cache_mode` | `auto` | `auto` uses split caches (db + runtime) |

### Optional Configuration
| Input | Default | Description |
|------|---------|-------------|
| `strict` | `false` | If true, treat warnings as errors |
| `junit` | `junit.xml` | JUnit report filename |
| `otel_jsonl` | `""` | OpenTelemetry JSONL output filename |
| `db` | `.eval/eval.db` | SQLite database path |

### Deprecated aliases (backwards compatible)

- `working_directory` → use `workdir`
- `upload_sarif` → use `sarif`
- `upload_exported_baseline` → use `upload_baseline_artifact`

## Golden-path examples

### 1. Gate PRs against baseline (Monorepo)
```yaml
- uses: Rul1an/verdict-action@v1
  with:
    verdict_version: v0.3.4
    workdir: packages/ai
    config: eval.yaml
    trace_file: traces/ci.jsonl
    baseline: baseline.json
    sarif: auto
```

### 2. Export baseline on main
```yaml
- uses: Rul1an/verdict-action@v1
  with:
    verdict_version: v0.3.4
    config: eval.yaml
    trace_file: traces/main.jsonl
    export_baseline: baseline.json
    upload_baseline_artifact: true
```

### 3. Caching Behavior
This action uses split caches to avoid "cache confusion":
- **DB cache**: `workdir/.eval` (incremental skip history)
- **Runtime caches**: `~/.verdict/cache` + `~/.verdict/embeddings` (precompute & performance)

Cache keys include OS, verdict version, and a hash of the workdir+config/trace to ensure safety.
