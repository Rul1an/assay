# Assay GitHub Action

> Zero-config evidence verification for AI agents. Native GitHub Security tab integration.

**Marketplace:** [assay-ai-agent-security](https://github.com/marketplace/actions/assay-ai-agent-security)
**Main Repository:** [Rul1an/assay](https://github.com/Rul1an/assay)

## Quick Start

```yaml
- uses: Rul1an/assay/assay-action@v2
```

That's it. Auto-discovers evidence bundles, verifies integrity, uploads findings to GitHub Security tab.

## What It Does

1. **Discovers** evidence bundles (`.assay/evidence/*.tar.gz`)
2. **Verifies** bundle integrity (content-addressed SHA-256)
3. **Lints** for security issues with optional compliance packs
4. **Reports** to GitHub Security tab + PR comments

## Copy-Paste Examples

### Basic (Auto-Discover + SARIF)

```yaml
name: Evidence Verification
on: [push, pull_request]

permissions:
  contents: read
  security-events: write

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: Rul1an/assay/assay-action@v2
```

### With EU AI Act Compliance Pack

```yaml
- uses: Rul1an/assay/assay-action@v2
  with:
    pack: eu-ai-act-baseline
```

Lints against Article 12 logging requirements. SARIF includes article references for audit trails.

### With Baseline Comparison

```yaml
- uses: Rul1an/assay/assay-action@v2
  with:
    baseline_key: main
    write_baseline: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
```

Detect regressions: new findings vs. your baseline. Baseline updates only on default branch.

### With BYOS (Bring Your Own Storage)

```yaml
permissions:
  contents: read
  id-token: write

steps:
  - uses: Rul1an/assay/assay-action@v2
    with:
      store: s3://my-bucket/evidence
      store_role: arn:aws:iam::123456789:role/AssayRole
```

Pushes evidence to your S3 bucket using OIDC. No static credentials. Also supports `gs://` (GCP) and `az://` (Azure).

### With Artifact Attestation

```yaml
permissions:
  contents: read
  attestations: write
  id-token: write

steps:
  - uses: Rul1an/assay/assay-action@v2
    with:
      attest: true
```

Generates SLSA-aligned provenance. Verify locally with `gh attestation verify bundle.tar.gz --owner OWNER`.

### Full Enterprise Pipeline

```yaml
name: Evidence Pipeline
on:
  push:
    branches: [main]
  pull_request:

permissions:
  contents: read
  security-events: write
  pull-requests: write
  attestations: write
  id-token: write

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Run tests
        run: |
          curl -fsSL https://getassay.dev/install.sh | sh
          assay run --policy policy.yaml -- pytest tests/

      - name: Verify & Report
        uses: Rul1an/assay/assay-action@v2
        with:
          pack: eu-ai-act-baseline
          store: s3://my-bucket/evidence
          store_role: ${{ secrets.AWS_ROLE_ARN }}
          attest: true
          baseline_key: main
          write_baseline: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
```

## Inputs

### Core (v2.0)

| Input | Default | Description |
|-------|---------|-------------|
| `bundles` | Auto | Glob pattern for evidence bundles |
| `fail_on` | `error` | Threshold: `error`, `warn`, `info`, `none` |
| `sarif` | `true` | Upload to GitHub Security tab |
| `comment_diff` | `true` | PR comment (only if findings) |
| `baseline_key` | - | Baseline comparison key |
| `write_baseline` | `false` | Save baseline (default branch only) |
| `version` | `latest` | CLI version |

### v2.1 Features

| Input | Default | Description |
|-------|---------|-------------|
| `pack` | - | Compliance pack(s): `eu-ai-act-baseline`, `./custom.yaml` |
| `store` | - | BYOS URL: `s3://`, `gs://`, `az://` |
| `store_role` | - | IAM role for OIDC auth |
| `attest` | `false` | Generate artifact attestation |
| `badge_gist` | - | Gist ID for coverage badge |

## Outputs

| Output | Description |
|--------|-------------|
| `verified` | `true` if all bundles verified |
| `findings_error` | Error count |
| `findings_warn` | Warning count |
| `reports_dir` | Reports directory path |
| `pack_score` | Compliance score (0-100) |
| `coverage_percent` | Evidence coverage % |
| `bundle_url` | BYOS URL (if pushed) |
| `attestation_url` | Attestation URL (if generated) |

## Permissions

```yaml
# Minimal (lint only)
permissions:
  contents: read

# SARIF + PR comments (recommended)
permissions:
  contents: read
  security-events: write
  pull-requests: write

# Full v2.1 (BYOS + attestation)
permissions:
  contents: read
  security-events: write
  pull-requests: write
  attestations: write
  id-token: write
```

## Security Model

- **Fork PRs**: Verify + lint only (no writes)
- **Same-repo PRs**: Verify + lint + SARIF + PR comment
- **Default branch push**: All features (baseline, BYOS, attestation, badge)

## Documentation

- [Full Guide](https://github.com/Rul1an/assay/blob/main/docs/guides/github-action.md)
- [SPEC-GitHub-Action-v2.1](https://github.com/Rul1an/assay/blob/main/docs/architecture/SPEC-GitHub-Action-v2.1.md)
- [Compliance Packs](https://github.com/Rul1an/assay/blob/main/docs/architecture/SPEC-Pack-Engine-v1.md)

## License

MIT
