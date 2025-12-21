# Verdict PR Gate (GitHub Action)

Marketplace-ready composite action that:
- downloads a pinned Verdict binary from GitHub Releases
- runs `verdict ci` (optionally in replay mode via `--trace-file`)
- uploads JUnit + SARIF + run artifacts
- optionally uploads SARIF to GitHub Code Scanning

## Usage

### Minimal (Replay mode / deterministic)
```yaml
name: Verdict CI

on:
  pull_request:
  push:
    branches: [ "main" ]

permissions:
  contents: read
  security-events: write

jobs:
  verdict:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - uses: verdict-eval/action@v1
        with:
          verdict_version: v0.2.0
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          redact_prompts: "true"
```

### Strict mode (Warn/Flaky become blocking)

```yaml
      - uses: verdict-eval/action@v1
        with:
          verdict_version: v0.2.0
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          strict: "true"
```

### Inputs (selected)

*   `verdict_version` (required): pinned release tag (e.g. `v0.2.0`)
*   `repo`: where releases live (default `Rul1an/verdict`)
*   `config`: eval config file (default `ci-eval.yaml`)
*   `trace_file`: JSONL traces for replay mode (default empty)
*   `strict`: `true`|`false` (default `false`)
*   `redact_prompts`: `true`|`false` (default `true`)
*   `upload_sarif`: `true`|`false` (default `true`)
*   `upload_artifacts`: `true`|`false` (default `true`)

### Required release assets

This action downloads a Verdict release asset:

`verdict-${os}-${arch}.tar.gz`

Examples:
*   `verdict-linux-x86_64.tar.gz`
*   `verdict-macos-aarch64.tar.gz`

The tarball must contain an executable named `verdict`.

If your asset name differs, pass `asset_name`:

```yaml
with:
  verdict_version: v0.2.0
  asset_name: verdict-x86_64-unknown-linux-musl.tar.gz
```

### Notes
*   On forked PRs, Code Scanning upload may be restricted by permissions. This action sets `continue-on-error: true` for SARIF upload to avoid blocking the job.
*   For best reproducibility, always pin `verdict_version` to a tag.
