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

      - uses: Rul1an/verdict-action@v1
        with:
          verdict_version: v0.1.0
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          redact_prompts: "true"
```

### Strict mode (Warn/Flaky become blocking)

```yaml
      - uses: Rul1an/verdict-action@v1
        with:
          verdict_version: v0.1.0
          config: ci-eval.yaml
          trace_file: traces/ci.jsonl
          strict: "true"
          upload_sarif: "true"
```

## Inputs (selected)
- `verdict_version` (required): pinned release tag (e.g. `v0.1.0`)
- `repo`: where releases live (default `Rul1an/verdict`)
- `config`: eval config file (default `ci-eval.yaml`)
- `trace_file`: JSONL traces for replay mode (default empty)
- `strict`: `true|false` (default `false`)
- `redact_prompts`: `true|false` (default `true`)
- `upload_sarif`: `true|false` (default `true`)
- `upload_artifacts`: `true|false` (default `true`)

## Permissions & Security
To enable **SARIF Upload** (GitHub Code Scanning integration), your generic `permissions` block must include:

```yaml
permissions:
  contents: read
  security-events: write # Required for upload-sarif
```

### Fork Behavior
The action automatically detects if it is running in a **Fork PR**.
- `upload_sarif` is intentionally skipped on forks to prevent permission errors, as forks generally do not have `security-events: write` access.
- Tests will still run, and `junit.xml` / `run.json` artifacts will be uploaded, ensuring contributors still get feedback.

## Required release assets

This action downloads a Verdict release asset:

`verdict-${os}-${arch}.tar.gz`

Examples:
- `verdict-linux-x86_64.tar.gz`
- `verdict-macos-aarch64.tar.gz`

> [!NOTE]
> Windows is currently **not supported**, as `verdict` is primarily tested on Linux/macOS.

The tarball must contain an executable named `verdict`.
