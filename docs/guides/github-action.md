# GitHub Action Integration

Assay provides a GitHub Action for automated evidence verification.

**GitHub Marketplace:** [Rul1an/assay-action](https://github.com/marketplace/actions/assay-ai-agent-security)

## Quick Start

```yaml
- uses: Rul1an/assay-action@v2
```

Zero config. Discovers evidence bundles, verifies integrity, uploads SARIF.

## What It Does

1. **Discovers** evidence bundles in your repo (`.assay/evidence/*.tar.gz`)
2. **Verifies** bundle integrity (content-addressed IDs)
3. **Lints** for security issues with optional compliance packs
4. **Reports** to GitHub Security tab + PR comments

## Full Workflow Example

```yaml
# .github/workflows/assay.yaml
name: Evidence Verification

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

      - name: Run tests with evidence collection
        run: |
          curl -fsSL https://getassay.dev/install.sh | sh
          echo "$HOME/.local/bin" >> $GITHUB_PATH
          assay run --policy policy.yaml -- pytest tests/

      - name: Verify evidence
        uses: Rul1an/assay-action@v2
        with:
          fail_on: error
          baseline_key: ${{ github.event.repository.name }}
          write_baseline: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
```

## Inputs

### Core Inputs (v2.0)

| Input | Default | Description |
|-------|---------|-------------|
| `bundles` | Auto-detect | Glob pattern for evidence bundles |
| `fail_on` | `error` | Fail threshold: `error`, `warn`, `info`, `none` |
| `sarif` | `true` | Upload to GitHub Security tab |
| `comment_diff` | `true` | Post PR comment (only if findings) |
| `baseline_key` | - | Key for baseline comparison |
| `write_baseline` | `false` | Save baseline (default branch only) |
| `version` | `latest` | Assay CLI version |

### Compliance Pack Input (v2.1)

| Input | Default | Description |
|-------|---------|-------------|
| `pack` | - | Compliance pack(s): `eu-ai-act-baseline`, `soc2-baseline`, `./custom.yaml` |

### BYOS Input (v2.1)

| Input | Default | Description |
|-------|---------|-------------|
| `store` | - | BYOS URL: `s3://bucket/prefix`, `gs://bucket`, `az://container` |
| `store_provider` | `auto` | `aws`, `gcp`, `azure`, or `auto` |
| `store_role` | - | IAM role/identity for OIDC |

### Attestation Input (v2.1)

| Input | Default | Description |
|-------|---------|-------------|
| `attest` | `false` | Generate artifact attestation |

## Outputs

| Output | Description |
|--------|-------------|
| `verified` | `true` if all bundles verified |
| `findings_error` | Error count |
| `findings_warn` | Warning count |
| `reports_dir` | Reports directory path |
| `pack_applied` | Applied pack IDs (v2.1) |
| `pack_score` | Compliance score 0-100 (v2.1) |
| `bundle_url` | BYOS bundle URL (v2.1) |
| `attestation_id` | Attestation UUID (v2.1) |

## Permission Model

```yaml
# Minimal (lint only)
permissions:
  contents: read

# With SARIF upload
permissions:
  contents: read
  security-events: write

# With PR comments
permissions:
  contents: read
  security-events: write
  pull-requests: write

# With attestation + OIDC (v2.1)
permissions:
  contents: read
  security-events: write
  attestations: write
  id-token: write
```

## Compliance Packs

Lint evidence against regulatory requirements:

```yaml
- uses: Rul1an/assay-action@v2
  with:
    pack: eu-ai-act-baseline
```

SARIF output includes article references (`Article 12(1)`, etc.) for audit trails.

**Available packs:**

| Pack | Coverage |
|------|----------|
| `eu-ai-act-baseline` | Article 12 logging requirements |
| `soc2-baseline` | Control mapping (coming soon) |

Custom packs:

```yaml
- uses: Rul1an/assay-action@v2
  with:
    pack: ./my-org-rules.yaml
```

## BYOS Push with OIDC

Push evidence to your own storage. No static credentials.

### AWS S3

```yaml
permissions:
  id-token: write
  contents: read

jobs:
  assay:
    runs-on: ubuntu-latest
    steps:
      - uses: Rul1an/assay-action@v2
        with:
          store: s3://my-bucket/evidence
          store_provider: aws
          store_role: arn:aws:iam::123456789:role/assay-evidence-push
```

Requires IAM trust policy for `token.actions.githubusercontent.com`.

### GCP Cloud Storage

```yaml
- uses: Rul1an/assay-action@v2
  with:
    store: gs://my-bucket/evidence
    store_provider: gcp
    store_role: projects/my-project/locations/global/workloadIdentityPools/github/providers/github
```

## Artifact Attestation

Generate SLSA-aligned provenance for evidence bundles:

```yaml
permissions:
  attestations: write
  id-token: write

steps:
  - uses: Rul1an/assay-action@v2
    with:
      attest: true
```

Verify locally:

```bash
gh attestation verify bundle.tar.gz --owner Rul1an
```

Attestations only run on push to default branch (security).

## Examples

### Baseline Comparison

Detect regressions against your default branch:

```yaml
- uses: Rul1an/assay-action@v2
  with:
    baseline_key: unit-tests
    write_baseline: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
```

### Matrix Builds

Multiple test suites:

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

### Full v2.1 Workflow

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

      - name: Verify with compliance pack
        uses: Rul1an/assay-action@v2
        with:
          pack: eu-ai-act-baseline
          store: s3://my-bucket/evidence
          store_provider: aws
          store_role: ${{ secrets.AWS_ROLE_ARN }}
          attest: true
          baseline_key: main
          write_baseline: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
```

## Manual CLI Workflow

Using the CLI directly:

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

      - name: Lint with pack
        run: assay evidence lint --pack eu-ai-act-baseline --format sarif --output results.sarif
        continue-on-error: true

      - name: Upload SARIF
        uses: github/codeql-action/upload-sarif@v4
        if: always()
        with:
          sarif_file: results.sarif
```

## Troubleshooting

### No evidence bundles found

The action looks for:
- `.assay/evidence/*.tar.gz`
- `evidence/*.tar.gz`

Generate with:

```bash
assay run --policy policy.yaml -- your-test-command
```

### SARIF upload fails

Check `security-events: write` permission.

### PR comments not appearing

Check `pull-requests: write` permission and that the action runs on a `pull_request` event.

### OIDC authentication fails

Verify IAM trust relationship includes your repo:

```json
{
  "Condition": {
    "StringLike": {
      "token.actions.githubusercontent.com:sub": "repo:YOUR-ORG/YOUR-REPO:*"
    }
  }
}
```

## Security Notes

- **Write operations** (baseline, BYOS push, attestation) only run on push to default branch
- **Fork PRs** cannot trigger write operations (GitHub Actions security model)
- **BYOS push** requires OIDC trust relationship configured in cloud IAM

## Related

- [Evidence Bundles](../concepts/traces.md)
- [Compliance Packs](../architecture/SPEC-Pack-Engine-v1.md)
- [Tool Signing](../architecture/SPEC-Tool-Signing-v1.md)
- [ADR-018: Action v2.1](../architecture/ADR-018-GitHub-Action-v2.1.md)
