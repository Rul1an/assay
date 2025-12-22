# Verdict PR Gate (GitHub Action)

Marketplace-ready composite action that:
- downloads a pinned Verdict binary from GitHub Releases
- runs `verdict ci` (optionally in replay mode via `--trace-file`)
- uploads JUnit + SARIF + run artifacts + exported baselines
- optionally uploads SARIF to GitHub Code Scanning

## Usage

### 1. PR Gate (Compare against Baseline)
Run checks and gate against a `baseline.json` (committed in repo).

```yaml
name: Verdict CI
on: [pull_request]
jobs:
  verdict:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write # For SARIF
    steps:
      - uses: actions/checkout@v4
      - uses: verdict-eval/action@v1
        with:
          verdict_version: v0.2.0
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          baseline: baseline.json # <--- Compare stats against this
```

### 2. Main Branch (Export Baseline)
Run checks on main and generate a fresh `baseline.json` artifact (to be merged or used by PRs).

```yaml
name: Verdict Baseline Export
on:
  push:
    branches: [ "main" ]
jobs:
  export:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: verdict-eval/action@v1
        with:
          verdict_version: v0.2.0
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          export_baseline: baseline.json # <--- Generate new baseline
          upload_artifacts: true
```

### Inputs

| Input | Description | Default |
| :--- | :--- | :--- |
| `verdict_version` | **Required**. Release tag to download (e.g. `v0.2.0`). | |
| `repo` | GitHub repo for releases. | `Rul1an/verdict` |
| `config` | Eval YAML config path. | `ci-eval.yaml` |
| `trace_file` | JSONL trace for replay mode. | `""` |
| `baseline` | Path to known-good baseline JSON (for gating). | `""` |
| `export_baseline` | Path to write new baseline JSON to. | `""` |
| `cache_mode` | Cache strategy: `off`, `verdict-cache`. | `verdict-cache` |
| `strict` | If `true`, exit 1 on warnings/flakes. | `false` |
| `redact_prompts` | Redact PII from outputs. | `true` |
| `upload_sarif` | Upload to GitHub Code Scanning. | `true` |
| `upload_artifacts` | Upload reports/baseline as artifacts. | `true` |
| `asset_name` | Override binary filename. | `""` |

### Caching

By default (`cache_mode: verdict-cache`), this action caches the `.eval` directory. This speeds up runs by persisting:
*   LLM Responses (HTTP Cache)
*   Embeddings (Vector Store)

The cache key is derived from: `verdict-{os}-{version}-{hash(config)}`. If the config changes, a fresh cache is built (but partial hits may be restored via `restore-keys`).
To disable caching, set `cache_mode: off`.

### Required release assets

This action downloads a Verdict release asset:
`verdict-${os}-${arch}.tar.gz`

Examples:
*   `verdict-linux-x86_64.tar.gz`
*   `verdict-macos-aarch64.tar.gz`

The tarball must contain an executable named `verdict`.

### Notes
*   On forked PRs, Code Scanning upload may be restricted by permissions. This action sets `continue-on-error: true` for SARIF upload.
*   For best reproducibility, always pin `verdict_version`.
