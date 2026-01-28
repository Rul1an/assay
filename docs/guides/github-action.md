# GitHub Action Integration

Assay provides a GitHub Action for automated AI agent security verification.

**GitHub Marketplace:** [Rul1an/assay-action](https://github.com/marketplace/actions/assay-ai-agent-security)

## Quick Start

```yaml
- uses: Rul1an/assay-action@v2
```

That's it. Zero config.

## What It Does

1. **Discovers** evidence bundles in your repo (`.assay/evidence/*.tar.gz`)
2. **Verifies** bundle integrity (cryptographic proof)
3. **Lints** for security issues (unauthorized file access, network calls, shell commands)
4. **Reports** results to GitHub Security tab + PR comments

## Full Workflow Example

```yaml
# .github/workflows/assay.yaml
name: AI Agent Security

on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read
  security-events: write
  pull-requests: write

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run tests with Assay
        run: |
          # Install Assay CLI
          curl -fsSL https://getassay.dev/install.sh | sh
          echo "$HOME/.local/bin" >> $GITHUB_PATH

          # Run your tests with evidence collection
          assay run --policy policy.yaml -- pytest tests/

      - name: Verify AI agent behavior
        uses: Rul1an/assay-action@v2
        with:
          fail_on: error
          baseline_key: ${{ github.event.repository.name }}
          write_baseline: ${{ github.ref == 'refs/heads/main' }}
```

## Inputs

| Input | Default | Description |
|-------|---------|-------------|
| `bundles` | Auto-detect | Glob pattern for evidence bundles |
| `fail_on` | `error` | Fail threshold: `error`, `warn`, `info`, `none` |
| `sarif` | `true` | Upload to GitHub Security tab |
| `comment_diff` | `true` | Post PR comment (only if findings) |
| `baseline_key` | - | Key for baseline comparison |
| `write_baseline` | `false` | Save baseline (main branch only) |

## Outputs

| Output | Description |
|--------|-------------|
| `verified` | `true` if all bundles verified |
| `findings_error` | Count of error-level findings |
| `findings_warn` | Count of warning-level findings |
| `reports_dir` | Path to reports directory |

## Permissions Required

```yaml
permissions:
  contents: read          # Checkout
  security-events: write  # SARIF upload
  pull-requests: write    # PR comments (optional)
```

## Advanced Examples

### Baseline Comparison

Detect regressions against your main branch:

```yaml
- uses: Rul1an/assay-action@v2
  with:
    baseline_key: unit-tests
    write_baseline: ${{ github.ref == 'refs/heads/main' }}
```

### Custom Threshold

Allow warnings but fail on errors:

```yaml
- uses: Rul1an/assay-action@v2
  with:
    fail_on: error  # 'warn' would fail on warnings too
```

### Skip SARIF Upload

If you only want PR comments:

```yaml
- uses: Rul1an/assay-action@v2
  with:
    sarif: false
```

### Matrix Builds

For multiple test suites:

```yaml
jobs:
  test:
    strategy:
      matrix:
        suite: [unit, integration, e2e]
    steps:
      - uses: actions/checkout@v4

      - name: Run ${{ matrix.suite }} tests
        run: assay run --policy policy.yaml -- pytest tests/${{ matrix.suite }}

      - uses: Rul1an/assay-action@v2
        with:
          bundles: '.assay/evidence/${{ matrix.suite }}/*.tar.gz'
```

## Manual CLI Workflow

If you prefer using the CLI directly instead of the action:

```yaml
jobs:
  assay:
    runs-on: ubuntu-latest
    permissions:
      contents: read
      security-events: write

    steps:
      - uses: actions/checkout@v4

      - name: Install Assay
        run: |
          curl -fsSL https://getassay.dev/install.sh | sh
          echo "$HOME/.local/bin" >> $GITHUB_PATH

      - name: Run tests
        run: assay run --policy policy.yaml -- pytest tests/

      - name: Export evidence
        run: assay evidence export --output evidence.tar.gz

      - name: Lint evidence (SARIF)
        run: assay evidence lint --format sarif --output results.sarif
        continue-on-error: true

      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v4
        if: always()
        with:
          sarif_file: results.sarif
```

## Troubleshooting

### No evidence bundles found

The action looks for bundles in:
- `.assay/evidence/*.tar.gz`
- `evidence/*.tar.gz`

Generate bundles with:

```bash
assay run --policy policy.yaml -- your-test-command
```

### SARIF upload fails

Ensure you have `security-events: write` permission:

```yaml
permissions:
  security-events: write
```

### PR comments not appearing

Ensure you have `pull-requests: write` permission and the action runs on a PR event.

## Related

- [Assay CLI Documentation](../getting-started/quickstart.md)
- [Evidence Bundles](../concepts/traces.md)
- [Policy Configuration](../reference/policies.md)
