# ADR-018: GitHub Action v2.1 - Attestation, OIDC & Compliance

**Status:** Accepted (implemented v2.12.0)
**Date:** 2026-01-29
**Deciders:** @Rul1an
**Supersedes:** Extends ADR-014 (GitHub Action v2)

> **Implementation**: See [SPEC-GitHub-Action-v2.1](./SPEC-GitHub-Action-v2.1.md) for full specification.
> **Contract Tests**: `.github/workflows/action-tests.yml` validates all v2.1 features.

## Context

GitHub Action v2.0 established the foundation for evidence verification and SARIF integration. Several developments in the GitHub Actions ecosystem and our own mandate evidence work (v2.11.0) create opportunities for v2.1:

1. **Artifact Attestations (GA)**: GitHub's `actions/attest-build-provenance@v3` enables native SLSA-aligned provenance signing
2. **OIDC Authentication**: Zero-credential cloud authentication is now best practice for BYOS push
3. **Pack Engine (v2.10.0)**: Compliance packs with article references are ready for Action integration
4. **Mandate Evidence (v2.11.0)**: Cryptographic authorization trails strengthen enterprise compliance story

### Problem Statement

Current v2.0 limitations:
- No artifact provenance (bundles are unsigned)
- BYOS push requires static credentials (secrets rotation burden)
- Compliance packs not exposed in Action interface
- No coverage/compliance badges

## Decision

### Core Additions for v2.1

| Feature | Priority | Rationale |
|---------|----------|-----------|
| Compliance Pack Support | P1 | EU AI Act compliance story, high leverage |
| BYOS Push with OIDC | P2 | Zero-credential enterprise posture |
| Artifact Attestation | P3 | Supply chain integrity, audit trail completion |
| Coverage Badge | P4 | Developer DX, repo visibility |

### Threat Model: Fork PRs and Write Operations

**Critical principle**: Write operations MUST NOT run on `pull_request` from forks.

| Operation | `pull_request` (fork) | `pull_request` (same repo) | `push` (main) |
|-----------|----------------------|---------------------------|---------------|
| Verify + Lint | ✅ | ✅ | ✅ |
| SARIF Upload | ❌ (no permission) | ✅ | ✅ |
| PR Comment | ❌ | ✅ | N/A |
| Baseline Write | ❌ | ❌ | ✅ |
| BYOS Push | ❌ | ❌ | ✅ |
| Attestation | ❌ | ❌ | ✅ |
| Badge Update | ❌ | ❌ | ✅ |

**Implementation**: All write steps have explicit conditionals using default branch detection:

```yaml
# Use default branch detection (works with main, master, or custom default)
if: |
  github.event_name == 'push' &&
  github.ref == format('refs/heads/{0}', github.event.repository.default_branch)
```

> **Why not hardcode `main`**: Repos may use `master`, `trunk`, or custom default branches.
> Using `github.event.repository.default_branch` ensures portability.

### Permission Model (Minimal by Default)

```yaml
# Default (lint-only)
permissions:
  contents: read

# With SARIF upload
permissions:
  contents: read
  security-events: write

# With attestation + OIDC
permissions:
  contents: read
  security-events: write
  attestations: write
  id-token: write
  packages: write  # Required for container attestations (optional)

# With PR comment
permissions:
  contents: read
  pull-requests: write
```

**Principle**: Action documents required permissions per feature; users enable incrementally.

> **Note**: The `actions/attest-build-provenance` action requires `attestations: write` and
> `id-token: write`. For container images, `packages: write` is also needed.

### New Input Contract (v2.1)

```yaml
inputs:
  # ============ Existing (v2.0) ============
  bundles:
    description: 'Glob pattern for evidence bundles'
    default: ''
  fail_on:
    description: 'Fail threshold: error, warn, info, none'
    default: 'error'
  sarif:
    description: 'Upload SARIF to GitHub Security tab'
    default: 'true'
  category:
    description: 'SARIF category (auto-generated if omitted)'
    default: ''
  baseline_dir:
    description: 'Path to baseline bundles for diff'
    default: ''
  baseline_key:
    description: 'Key for baseline cache lookup'
    default: ''
  write_baseline:
    description: 'Write baseline after successful run (main branch only)'
    default: 'false'
  comment_diff:
    description: 'Post PR comment with diff summary'
    default: 'true'
  version:
    description: 'Assay CLI version to install'
    default: 'latest'

  # ============ New (v2.1) ============
  pack:
    description: |
      Compliance pack(s) to apply (comma-separated).
      Examples: eu-ai-act-baseline, soc2-baseline, ./custom.yaml
    required: false
    default: ''

  store:
    description: |
      BYOS store URL for evidence push.
      Examples: s3://bucket/prefix, az://container, gs://bucket
      Requires OIDC trust relationship configured.
    required: false
    default: ''

  store_provider:
    description: |
      Cloud provider for OIDC authentication.
      Options: aws, gcp, azure, auto (detect from URL)
    required: false
    default: 'auto'

  store_role:
    description: |
      IAM role/identity for OIDC authentication.
      AWS: arn:aws:iam::ACCOUNT:role/ROLE
      GCP: projects/PROJECT/locations/global/workloadIdentityPools/POOL/providers/PROVIDER
      Azure: azure://TENANT/APP
    required: false
    default: ''

  attest:
    description: |
      Generate SLSA-aligned artifact attestation for evidence bundles.
      Requires permissions: attestations: write, id-token: write
      Only runs on push to default branch.
    required: false
    default: 'false'

  badge_gist:
    description: |
      Gist ID for dynamic coverage badge.
      Requires GIST_TOKEN secret with gist:write scope.
      Only runs on push to default branch.
    required: false
    default: ''
```

### New Output Contract (v2.1)

```yaml
outputs:
  # ============ Existing (v2.0) ============
  verified:
    description: 'true if all bundles passed verification'
  findings_error:
    description: 'Count of error-level findings'
  findings_warn:
    description: 'Count of warning-level findings'
  sarif_path:
    description: 'Path to generated SARIF file'
  sarif_uploaded:
    description: 'true if SARIF was uploaded to Code Scanning'
  diff_summary:
    description: 'One-line diff summary'
  diff_new_findings:
    description: 'Count of new findings vs baseline'
  reports_dir:
    description: 'Path to reports directory'

  # ============ New (v2.1) ============
  pack_applied:
    description: 'Comma-separated list of applied pack IDs'
  pack_score:
    description: 'Compliance score (0-100) across all packs'
  pack_articles:
    description: 'Comma-separated list of covered articles (e.g., "12(1),12(2)(a)")'
  bundle_url:
    description: 'URL of pushed evidence bundle in BYOS (if store set)'
  attestation_id:
    description: 'Artifact attestation UUID (if attest=true)'
  attestation_url:
    description: 'URL to view attestation in GitHub UI (if attest=true)'
  coverage_percent:
    description: 'Evidence coverage percentage (tools with policy / total tools)'
```

### P1: Compliance Pack Support

**Implementation:**

```yaml
- name: Lint with compliance packs
  if: inputs.pack != ''
  shell: bash
  run: |
    PACKS="${{ inputs.pack }}"

    assay evidence lint \
      --format sarif \
      --pack "$PACKS" \
      --output "$REPORTS_DIR/lint.sarif" \
      $BUNDLES

    # Extract pack metadata for Job Summary
    SARIF="$REPORTS_DIR/lint.sarif"

    PACK_APPLIED=$(jq -r '[.runs[0].tool.driver.rules[]?.properties.pack // empty] | unique | join(",")' "$SARIF")
    PACK_SCORE=$(jq -r '.runs[0].properties.complianceScore // 100' "$SARIF")
    PACK_ARTICLES=$(jq -r '[.runs[0].tool.driver.rules[]?.properties.article_ref // empty] | unique | join(",")' "$SARIF")
    DISCLAIMER=$(jq -r '.runs[0].properties.disclaimer // empty' "$SARIF")

    echo "pack_applied=$PACK_APPLIED" >> $GITHUB_OUTPUT
    echo "pack_score=$PACK_SCORE" >> $GITHUB_OUTPUT
    echo "pack_articles=$PACK_ARTICLES" >> $GITHUB_OUTPUT

    # Store disclaimer for Job Summary
    if [ -n "$DISCLAIMER" ]; then
      echo "pack_disclaimer<<EOF" >> $GITHUB_OUTPUT
      echo "$DISCLAIMER" >> $GITHUB_OUTPUT
      echo "EOF" >> $GITHUB_OUTPUT
    fi
```

**SARIF Contract (per SPEC-Pack-Engine-v1):**

```json
{
  "runs": [{
    "tool": {
      "driver": {
        "name": "assay-evidence",
        "rules": [{
          "id": "eu-ai-act-baseline@1.0.0:EU12-001",
          "properties": {
            "pack": "eu-ai-act-baseline",
            "pack_version": "1.0.0",
            "article_ref": "Article 12(1)"
          }
        }]
      }
    },
    "properties": {
      "disclaimer": "This pack provides guidance only...",
      "complianceScore": 85
    }
  }]
}
```

**Job Summary Enhancement:**

The Job Summary MUST display disclaimer when present in SARIF:

```yaml
- name: Write Job Summary
  shell: bash
  run: |
    {
      echo "## Compliance Pack Results"
      echo ""
      echo "| Pack | Version | Score | Articles |"
      echo "|------|---------|-------|----------|"
      echo "| $PACK_APPLIED | 1.0.0 | ${PACK_SCORE}% | $PACK_ARTICLES |"

      # MANDATORY: Display disclaimer if present
      if [ -n "$DISCLAIMER" ]; then
        echo ""
        echo "> ⚠️ **Disclaimer**: $DISCLAIMER"
      fi
    } >> $GITHUB_STEP_SUMMARY
```

**Disclaimer requirement (NORMATIVE):**
- If `runs[0].properties.disclaimer` is present in SARIF, Job Summary MUST display it
- This is enforced by Pack Engine for `pack_kind == compliance`
- Failure to display disclaimer is a compliance risk

### P2: BYOS Push with OIDC

**Concurrency control (recommended for workflows using BYOS push):**

```yaml
# In calling workflow, add concurrency group to prevent parallel writes
concurrency:
  group: assay-evidence-${{ github.ref }}
  cancel-in-progress: false  # Don't cancel in-progress evidence push
```

> **Why**: Parallel pushes to BYOS may race on baseline updates or cause duplicate bundles.
> Concurrency group ensures sequential execution per branch.

**Provider-specific authentication (explicit, tested):**

```yaml
# AWS OIDC
- name: Configure AWS credentials (OIDC)
  if: inputs.store != '' && inputs.store_provider == 'aws'
  uses: aws-actions/configure-aws-credentials@e3dd6a429d7300a6a4c196c26e071d42e0343502 # v4.0.2
  with:
    role-to-assume: ${{ inputs.store_role }}
    aws-region: ${{ inputs.store_region || 'us-east-1' }}

# GCP OIDC
- name: Configure GCP credentials (OIDC)
  if: inputs.store != '' && inputs.store_provider == 'gcp'
  uses: google-github-actions/auth@6fc4af4b145ae7821d527454aa9bd537d1f2dc5f # v2.1.7
  with:
    workload_identity_provider: ${{ inputs.store_role }}

# Azure OIDC
- name: Configure Azure credentials (OIDC)
  if: inputs.store != '' && inputs.store_provider == 'azure'
  uses: azure/login@a65d910e8af852a8061c627c456678983e180302 # v2.2.0
  with:
    client-id: ${{ inputs.azure_client_id }}
    tenant-id: ${{ inputs.azure_tenant_id }}
    subscription-id: ${{ inputs.azure_subscription_id }}
```

**Push step (default branch only):**

```yaml
- name: Push evidence to BYOS
  if: |
    inputs.store != '' &&
    github.event_name == 'push' &&
    github.ref == format('refs/heads/{0}', github.event.repository.default_branch) &&
    steps.process.outputs.verified == 'true'
  shell: bash
  run: |
    for bundle in $BUNDLES; do
      URL=$(assay evidence push "$bundle" --store "${{ inputs.store }}" --json | jq -r '.url')
      echo "Pushed: $URL"
    done
    echo "bundle_url=$URL" >> $GITHUB_OUTPUT
```

### P3: Artifact Attestation

**Important clarification**: Artifact attestations provide strong provenance guarantees. Combined with isolated build environments, they contribute toward SLSA Build Level requirements. However, achieving a specific SLSA level requires meeting all criteria for that level, including builder hardening requirements beyond attestations alone.

**Implementation:**

```yaml
- name: Generate artifact attestation
  id: attest
  if: |
    inputs.attest == 'true' &&
    github.event_name == 'push' &&
    github.ref == format('refs/heads/{0}', github.event.repository.default_branch) &&
    steps.process.outputs.verified == 'true'
  uses: actions/attest-build-provenance@1c608d11d69870c2092266b3f9a6f3abbf17002c # v3.0.0
  with:
    subject-path: ${{ steps.process.outputs.reports_dir }}/*.tar.gz

- name: Export attestation outputs
  if: steps.attest.outcome == 'success'
  shell: bash
  run: |
    echo "attestation_id=${{ steps.attest.outputs.attestation-id }}" >> $GITHUB_OUTPUT
    echo "attestation_url=${{ steps.attest.outputs.attestation-url }}" >> $GITHUB_OUTPUT
```

**Action Outputs:**

| Output | Description |
|--------|-------------|
| `attestation-id` | UUID of the attestation |
| `attestation-url` | URL to view attestation in GitHub UI |

**Verification (user-side):**

```bash
gh attestation verify bundle.tar.gz --owner Rul1an
```

**Integration with mandate signatures:**

Evidence bundles contain:
1. **Bundle digest**: Content-addressed SHA256
2. **Mandate signatures**: DSSE/Ed25519 per mandate (v2.11.0)
3. **Artifact attestation**: GitHub-signed provenance (v2.1)

This creates an end-to-end integrity chain from user authorization to CI/CD output.

### P4: Coverage Badge

**Security consideration**: Requires `GIST_TOKEN` secret with minimal scope (`gist` only). Only runs on default branch to prevent exfiltration.

```yaml
- name: Update coverage badge
  if: |
    inputs.badge_gist != '' &&
    github.event_name == 'push' &&
    github.ref == format('refs/heads/{0}', github.event.repository.default_branch)
  uses: schneegans/dynamic-badges-action@e9a478b16159b4d31420099ba146cdc50f134483 # v1.7.0
  with:
    auth: ${{ secrets.GIST_TOKEN }}
    gistID: ${{ inputs.badge_gist }}
    filename: assay-coverage.json
    label: Evidence Coverage
    message: ${{ steps.process.outputs.coverage_percent }}%
    valColorRange: ${{ steps.process.outputs.coverage_percent }}
    maxColorRange: 100
    minColorRange: 0
```

**GIST_TOKEN requirements:**
- Fine-grained PAT with `gist` scope only
- Scope limited to single gist if possible
- Never used on fork PRs (default branch guard enforces this)

### EU AI Act Timeline

The EU AI Act (Regulation 2024/1689) has a **phased implementation schedule**. Obligations apply at different times depending on AI system classification.

| Date | Milestone | Scope |
|------|-----------|-------|
| Aug 2024 | Entry into force | Regulation published, transition begins |
| Feb 2025 | Chapter I-II apply | Prohibited practices (Art. 5), AI literacy (Art. 4) |
| Aug 2025 | Chapter III applies | General-purpose AI (GPAI) model obligations |
| Aug 2026 | High-risk obligations | Full Annex III compliance for high-risk AI |
| Aug 2027 | Extended scope | Certain Annex I systems |

**Important**: The timeline above is a summary. Specific obligations may have different effective dates based on system classification, sector, and transitional provisions. Always consult the official regulation text and legal counsel.

**Pack scope mapping:**

| Pack | Scope | Effective |
|------|-------|-----------|
| `eu-ai-act-baseline` | Article 12 (automatic logging) | Applies to all AI systems with logging obligations |
| `eu-ai-act-gpai` (future) | GPAI provider obligations | Aug 2025+ |
| `eu-ai-act-high-risk` (future) | Full Annex III requirements | Aug 2026+ |

**Pack requirements (NORMATIVE):**
1. Each pack MUST specify which articles it covers in `rules[].properties.article_ref`
2. Each pack MUST include effective dates in documentation
3. Compliance packs MUST include disclaimer per ADR-016
4. Pack version MUST be included in SARIF output for audit traceability

**Messaging guidance**: Never claim "EU AI Act compliant" without specifying:
- Which articles/obligations are covered
- Which AI system classification applies
- Effective dates of those obligations

### Supply Chain Hardening

All third-party actions pinned to commit SHA:

```yaml
# Verified and pinned (Jan 2026)
actions/cache@0c907a75c2c80ebcb7f088228285e798b750cf8f # v4.2.1
actions/upload-artifact@65c4c4a1ddee5b72f698fdd19549f0f0fb45cf08 # v4.6.0
github/codeql-action/upload-sarif@b20883b0cd1f46c72ae0ba6d1090936928f9fa30 # v4.32.0
actions/attest-build-provenance@1c608d11d69870c2092266b3f9a6f3abbf17002c # v3.0.0
aws-actions/configure-aws-credentials@e3dd6a429d7300a6a4c196c26e071d42e0343502 # v4.0.2
google-github-actions/auth@6fc4af4b145ae7821d527454aa9bd537d1f2dc5f # v2.1.7
schneegans/dynamic-badges-action@e9a478b16159b4d31420099ba146cdc50f134483 # v1.7.0
peter-evans/find-comment@3eae4d37986fb5a8592848f6a574fdf654e61f9e # v3.1.0
peter-evans/create-or-update-comment@71345be0265236311c031f5c7866368bd1ebb043 # v4.0.0
```

## Rationale

### Why OIDC over Static Credentials

| Factor | Static Credentials | OIDC |
|--------|-------------------|------|
| Secret rotation | Manual, error-prone | Automatic (short-lived tokens) |
| Blast radius | Full access until revoked | ~15 min token lifetime |
| Audit trail | Limited | Full GitHub → cloud correlation |
| Enterprise adoption | Barrier | Expected standard |

### Why Explicit Provider Configuration

Auto-detecting provider from URL is convenient but:
- Reduces debuggability
- May select wrong auth method
- Harder to document required IAM setup

Decision: `store_provider: auto` as default with **fail-closed** behavior:

**Auto-detection rules (fail-closed):**

| URL Prefix | Detected Provider | Required Input |
|------------|-------------------|----------------|
| `s3://` | `aws` | `store_role` (IAM role ARN) |
| `gs://` | `gcp` | `store_role` (Workload Identity) |
| `az://` or `https://*.blob.core.windows.net` | `azure` | Azure inputs |
| Other | **ERROR** | Must set `store_provider` explicitly |

**Fail-closed validation:**

```yaml
- name: Validate store configuration
  if: inputs.store != ''
  shell: bash
  run: |
    STORE="${{ inputs.store }}"
    PROVIDER="${{ inputs.store_provider }}"
    ROLE="${{ inputs.store_role }}"

    # Auto-detect provider if not set
    if [ "$PROVIDER" = "auto" ]; then
      case "$STORE" in
        s3://*) PROVIDER="aws" ;;
        gs://*) PROVIDER="gcp" ;;
        az://*|https://*.blob.core.windows.net/*) PROVIDER="azure" ;;
        *)
          echo "::error::Unknown store URL scheme. Set store_provider explicitly."
          exit 1
          ;;
      esac
    fi

    # Require store_role for OIDC providers
    if [ "$PROVIDER" = "aws" ] || [ "$PROVIDER" = "gcp" ]; then
      if [ -z "$ROLE" ]; then
        echo "::error::store_role is required for $PROVIDER OIDC authentication."
        echo "::error::AWS: arn:aws:iam::ACCOUNT:role/ROLE"
        echo "::error::GCP: projects/PROJECT/locations/global/workloadIdentityPools/POOL/providers/PROVIDER"
        exit 1
      fi
    fi

    echo "provider=$PROVIDER" >> $GITHUB_OUTPUT
```

### Why Not SLSA Level Claims

While attestations significantly improve supply chain integrity:
- SLSA levels have specific requirements beyond attestations
- "Level 3" claims require hardened builders with specific isolation properties
- GitHub-hosted runners provide good but not formally certified isolation

Decision: Document that attestations provide "SLSA-aligned provenance" without claiming specific levels.

## Implementation Plan

```
Week 1: P1 - Compliance Pack Support
├── Add `pack` input
├── Integrate `--pack` in lint step
├── Parse pack metadata from SARIF
├── Job Summary with disclaimer
└── Tests with eu-ai-act-baseline

Week 2: P2 - BYOS Push + OIDC
├── Add store inputs (store, store_provider, store_role)
├── AWS OIDC configuration step
├── GCP OIDC configuration step
├── Azure OIDC configuration step (optional)
├── `assay evidence push` integration
├── Main-branch-only conditional
└── E2E test with test bucket

Week 3: P3 - Artifact Attestation
├── Add `attest` input
├── Integrate attest-build-provenance@v3
├── Document permission requirements
├── Verification instructions
└── Integration test

Week 4: P4 - Badge + Polish
├── Badge generation via dynamic-badges-action
├── Security review (GIST_TOKEN scope)
├── Documentation update
├── Release notes
└── Marketplace update
```

## Consequences

### Positive

- **Compliance story**: Packs + Job Summary = auditor-friendly output
- **Zero-credential BYOS**: Enterprise-ready without secret rotation
- **Provenance chain**: Mandate signatures → bundle digest → attestation
- **Developer DX**: Coverage badges increase visibility

### Negative

- **Complexity**: More inputs, more conditionals, more documentation
- **Permission sprawl**: Users must understand which features need which permissions
- **OIDC setup**: Requires IAM configuration (one-time but non-trivial)

### Risks

| Risk | Mitigation |
|------|------------|
| OIDC misconfiguration | Clear error messages, setup guides per provider |
| Attestation failures | `continue-on-error: true` with warning |
| Badge token leak | Main-branch-only, minimal gist scope |
| Pack false positives | Disclaimer enforcement, article_ref clarity |

## References

### GitHub Documentation
- [Artifact Attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations)
- [SLSA Build Level 3 with Reusable Workflows](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations/using-artifact-attestations-and-reusable-workflows-to-achieve-slsa-v1-build-level-3)
- [OIDC with AWS](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-amazon-web-services)
- [OIDC with GCP](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-google-cloud-platform)
- [SARIF Support](https://docs.github.com/en/code-security/code-scanning/integrating-with-code-scanning/sarif-support-for-code-scanning)

### EU AI Act
- [EUR-Lex AI Act Full Text](https://eur-lex.europa.eu/eli/reg/2024/1689/oj)
- [European Commission AI Act Timeline](https://digital-strategy.ec.europa.eu/en/policies/regulatory-framework-ai)

### Internal References
- [ADR-014: GitHub Action v2](./ADR-014-GitHub-Action-v2.md)
- [ADR-016: Pack Taxonomy](./ADR-016-Pack-Taxonomy.md)
- [ADR-017: Mandate Evidence](./ADR-017-Mandate-Evidence.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- [SPEC-Mandate-v1](./SPEC-Mandate-v1.md)
