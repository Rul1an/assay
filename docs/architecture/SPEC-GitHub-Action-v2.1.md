# SPEC-GitHub-Action-v2.1

**Version:** 1.0.0
**Status:** Implemented (v2.12.0)
**Date:** 2026-01-29
**ADR:** [ADR-018](./ADR-018-GitHub-Action-v2.1.md)

> **DX Note**: See [Quick Start](#quick-start-dx) for copy-paste examples.

## Abstract

This specification defines the GitHub Action v2.1 interface, behavior, and implementation requirements. It covers compliance pack integration, BYOS push with OIDC authentication, artifact attestation, and coverage badge generation.

---

## Quick Start (DX)

### Zero-Config (Just Works)

```yaml
- uses: Rul1an/assay-action@v2
```

Auto-discovers evidence bundles, verifies integrity, uploads SARIF to Security tab.

### With EU AI Act Compliance Pack

```yaml
- uses: Rul1an/assay-action@v2
  with:
    pack: eu-ai-act-baseline
```

Lints evidence against Article 12 requirements. SARIF includes article references.

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

      - name: Run tests (generates evidence)
        run: |
          curl -fsSL https://getassay.dev/install.sh | sh
          assay run --policy policy.yaml -- pytest tests/

      - name: Verify & Report
        uses: Rul1an/assay-action@v2
        with:
          pack: eu-ai-act-baseline
          store: s3://my-bucket/evidence
          store_role: arn:aws:iam::123456789:role/AssayRole
          attest: true
          baseline_key: main
          write_baseline: ${{ github.ref == format('refs/heads/{0}', github.event.repository.default_branch) }}
```

---

---

## 1. Scope

### 1.1 In Scope

- Input/output contract for v2.1 features
- Permission requirements per feature
- Security model (fork PR threat model)
- OIDC authentication flow per cloud provider
- SARIF integration with pack metadata
- Job Summary format requirements

### 1.2 Out of Scope

- CLI implementation (`assay evidence lint`, `assay evidence push`)
- Pack Engine internals (see SPEC-Pack-Engine-v1)
- Mandate signing (see SPEC-Mandate-v1)

---

## 2. Terminology

| Term | Definition |
|------|------------|
| **Default Branch** | Repository's default branch (`main`, `master`, or custom) |
| **BYOS** | Bring Your Own Storage - user-provided S3/GCS/Azure storage |
| **OIDC** | OpenID Connect - federated authentication without static credentials |
| **Pack** | Compliance/security rule bundle (see SPEC-Pack-Engine-v1) |
| **Attestation** | Cryptographically signed provenance statement |

---

## 3. Input Contract

### 3.1 Existing Inputs (v2.0)

| Input | Type | Default | Description |
|-------|------|---------|-------------|
| `bundles` | glob | `''` | Evidence bundle pattern |
| `fail_on` | enum | `error` | `error`, `warn`, `info`, `none` |
| `sarif` | bool | `true` | Upload SARIF to Code Scanning |
| `category` | string | auto | SARIF category |
| `baseline_dir` | path | `''` | Baseline bundles path |
| `baseline_key` | string | `''` | Baseline cache key |
| `write_baseline` | bool | `false` | Write baseline on default branch |
| `comment_diff` | bool | `true` | Post PR comment |
| `version` | string | `latest` | Assay CLI version |

### 3.2 New Inputs (v2.1)

| Input | Type | Default | Required | Description |
|-------|------|---------|----------|-------------|
| `pack` | string | `''` | No | Comma-separated pack names or paths |
| `store` | string | `''` | No | BYOS URL (`s3://`, `gs://`, `az://`) |
| `store_provider` | enum | `auto` | No | `aws`, `gcp`, `azure`, `auto` |
| `store_role` | string | `''` | Conditional | IAM role/identity for OIDC (required for aws/gcp) |
| `store_region` | string | `us-east-1` | No | AWS region (AWS only) |
| `azure_client_id` | string | `''` | Conditional | Azure App Registration client ID (required for azure) |
| `azure_tenant_id` | string | `''` | Conditional | Azure AD tenant ID (required for azure) |
| `azure_subscription_id` | string | `''` | Conditional | Azure subscription ID (required for azure) |
| `attest` | bool | `false` | No | Generate artifact attestation |
| `badge_gist` | string | `''` | No | Gist ID for coverage badge |

### 3.3 Input Validation (NORMATIVE)

**3.3.1 Pack Input**

```
pack := pack_ref ("," pack_ref)*
pack_ref := pack_name | pack_path
pack_name := identifier "@" version | identifier
pack_path := "./" path_segment ("/" path_segment)* ".yaml"
```

Examples:
- `eu-ai-act-baseline`
- `eu-ai-act-baseline@1.0.0`
- `eu-ai-act-baseline,soc2-baseline`
- `./custom-pack.yaml`

**3.3.2 Store Input**

MUST match one of:
- `s3://bucket/prefix`
- `gs://bucket/prefix`
- `az://container/prefix`
- `https://*.blob.core.windows.net/container/prefix`

**3.3.3 Store Provider Auto-Detection**

| URL Pattern | Detected Provider |
|-------------|-------------------|
| `s3://` | `aws` |
| `gs://` | `gcp` |
| `az://` | `azure` |
| `*.blob.core.windows.net` | `azure` |
| Other | **ERROR** (fail-closed) |

**3.3.4 Store Role/Identity Requirement**

| Provider | Required Inputs | Format |
|----------|----------------|--------|
| `aws` | `store_role` | `arn:aws:iam::ACCOUNT:role/ROLE` |
| `gcp` | `store_role` | `projects/PROJECT/locations/global/workloadIdentityPools/POOL/providers/PROVIDER` |
| `azure` | `azure_client_id`, `azure_tenant_id`, `azure_subscription_id` | Standard Azure GUID format |

**3.3.5 Store URL Clarification**

| URL Pattern | Meaning |
|-------------|---------|
| `s3://bucket/prefix` | AWS S3 bucket with optional prefix |
| `gs://bucket/prefix` | Google Cloud Storage bucket |
| `az://container/prefix` | Azure Blob Storage (shorthand) |
| `https://ACCOUNT.blob.core.windows.net/CONTAINER/prefix` | Azure Blob Storage (full URL) |

> **Note**: `az://` is a convenience shorthand. When using `az://`, the action resolves to
> `https://{storage_account}.blob.core.windows.net/{container}` using the authenticated identity.

---

## 4. Output Contract

### 4.1 Existing Outputs (v2.0)

| Output | Type | Description |
|--------|------|-------------|
| `verified` | bool | All bundles verified |
| `findings_error` | int | Error count |
| `findings_warn` | int | Warning count |
| `sarif_path` | path | Generated SARIF |
| `sarif_uploaded` | bool | SARIF upload success |
| `diff_summary` | string | One-line diff |
| `diff_new_findings` | int | New vs baseline |
| `reports_dir` | path | Reports directory |

### 4.2 New Outputs (v2.1)

| Output | Type | Condition | Description |
|--------|------|-----------|-------------|
| `pack_applied` | string | `pack` set | Comma-separated pack IDs (input order) |
| `pack_score` | int | `pack` set | Compliance score (0-100, see §4.3) |
| `pack_articles` | string | `pack` set | Covered articles (union, sorted, deduped) |
| `bundle_url` | string | `store` set | Pushed bundle URL |
| `attestation_id` | string | `attest=true` | Attestation UUID |
| `attestation_url` | string | `attest=true` | Attestation view URL |
| `attestation_bundle_path` | string | `attest=true` | Local path to Sigstore bundle |
| `coverage_percent` | int | Always | Evidence coverage percentage (see §4.4) |

### 4.3 Multi-Pack Output Aggregation (NORMATIVE)

When multiple packs are specified (`--pack a,b,c`):

| Output | Aggregation Rule |
|--------|------------------|
| `pack_applied` | Comma-separated in input order |
| `pack_score` | **Minimum** score across all packs (conservative for compliance posture) |
| `pack_articles` | Union of all articles, sorted alphabetically, deduplicated |

**Rationale**: Using minimum score ensures the workflow fails when _any_ pack has low coverage,
which is the appropriate posture for compliance gating.

### 4.4 Coverage Definition (NORMATIVE)

**Evidence Coverage** measures how many observed tools have policy decisions recorded.
```
coverage_percent = floor((tools_with_decisions / tools_observed) * 100)
```

| Term | Definition |
|------|------------|
| `tools_observed` | Unique tool names in evidence bundles |
| `tools_with_decisions` | Tools that have at least one policy decision event |

**Multi-bundle aggregation**: When multiple bundles are processed, compute coverage across the
union of all bundles (deduplicated by tool name).

**Rounding**: Always round down (`floor`) to avoid overstating coverage.

---

## 5. Permission Model

### 5.1 Base Permissions

```yaml
permissions:
  contents: read
```

### 5.2 Feature-Specific Permissions

| Feature | Additional Permissions |
|---------|----------------------|
| SARIF Upload | `security-events: write` |
| PR Comment | `pull-requests: write` |
| Attestation | `attestations: write`, `id-token: write` |
| Attestation (recommended) | `packages: write` for container images |
| OIDC (any) | `id-token: write` |

> **Note on `packages: write`**: The `actions/attest-build-provenance` action can link attestations
> to container images when `packages: write` is granted. Without this permission, attestations are
> still created but lack the storage record linkage. For evidence bundles (non-container), this
> permission is optional.

### 5.3 Recommended Workflow Permissions

```yaml
# Full v2.1 feature set
permissions:
  contents: read
  security-events: write
  pull-requests: write
  attestations: write
  id-token: write
```

---

## 6. Security Model

### 6.1 Fork PR Threat Model

**Principle**: Write operations MUST NOT execute on fork PRs.

| Operation | Fork PR | Same-Repo PR | Default Branch Push |
|-----------|---------|--------------|---------------------|
| Verify | ✅ | ✅ | ✅ |
| Lint | ✅ | ✅ | ✅ |
| SARIF Upload | ❌ | ✅ | ✅ |
| PR Comment | ❌ | ✅ | N/A |
| Baseline Write | ❌ | ❌ | ✅ |
| BYOS Push | ❌ | ❌ | ✅ |
| Attestation | ❌ | ❌ | ✅ |
| Badge Update | ❌ | ❌ | ✅ |

### 6.2 Default Branch Guard (NORMATIVE)

All write operations MUST use default branch detection:

```yaml
if: |
  github.event_name == 'push' &&
  github.ref == format('refs/heads/{0}', github.event.repository.default_branch)
```

**Rationale**: Hardcoding `refs/heads/main` fails for repos using `master` or custom defaults.

### 6.3 OIDC Security

- Tokens are short-lived (~15 min)
- Trust relationship configured in cloud IAM
- No static credentials in repository secrets
- Full audit trail from GitHub to cloud provider

### 6.4 Badge Token Security

- `GIST_TOKEN` MUST be fine-grained PAT
- Scope MUST be limited to `gist` only
- Badge update MUST only run on default branch
- Consider: scope to specific gist if supported

---

## 7. Compliance Pack Integration

### 7.1 SARIF Contract

Packs produce SARIF with the following structure:

```json
{
  "$schema": "https://raw.githubusercontent.com/oasis-tcs/sarif-spec/master/Schemata/sarif-schema-2.1.0.json",
  "version": "2.1.0",
  "runs": [{
    "tool": {
      "driver": {
        "name": "assay-evidence",
        "version": "2.12.0",
        "rules": [{
          "id": "{pack}@{version}:{rule_id}",
          "name": "Rule Name",
          "properties": {
            "pack": "eu-ai-act-baseline",
            "pack_version": "1.0.0",
            "article_ref": "Article 12(1)"
          }
        }]
      }
    },
    "results": [...],
    "properties": {
      "disclaimer": "This pack provides guidance only...",
      "complianceScore": 85
    }
  }]
}
```

### 7.2 Disclaimer Requirement (NORMATIVE)

If `runs[0].properties.disclaimer` is present:
1. Job Summary MUST display it
2. PR Comment MUST include it (if posted)
3. Display MUST use warning formatting

### 7.3 Article Reference Format

`article_ref` SHOULD follow pattern:
- `Article N` (single article)
- `Article N(M)` (paragraph)
- `Article N(M)(x)` (subparagraph)

Examples: `Article 12(1)`, `Article 12(2)(a)`, `Article 5`

---

## 8. BYOS Push Flow

### 8.1 AWS OIDC Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   GitHub    │────▶│ AWS STS     │────▶│  S3 Bucket  │
│   Actions   │     │ AssumeRole  │     │             │
└─────────────┘     └─────────────┘     └─────────────┘
       │                   │                   │
       │  OIDC Token       │  Temp Creds       │  PutObject
       └───────────────────┴───────────────────┘
```

**IAM Trust Policy (user setup):**

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Principal": {
      "Federated": "arn:aws:iam::ACCOUNT:oidc-provider/token.actions.githubusercontent.com"
    },
    "Action": "sts:AssumeRoleWithWebIdentity",
    "Condition": {
      "StringEquals": {
        "token.actions.githubusercontent.com:aud": "sts.amazonaws.com"
      },
      "StringLike": {
        "token.actions.githubusercontent.com:sub": "repo:ORG/REPO:ref:refs/heads/main"
      }
    }
  }]
}
```

### 8.2 GCP OIDC Flow

**Workload Identity setup (user):**

```bash
gcloud iam workload-identity-pools create assay-pool \
  --location="global"

gcloud iam workload-identity-pools providers create-oidc github \
  --location="global" \
  --workload-identity-pool="assay-pool" \
  --issuer-uri="https://token.actions.githubusercontent.com" \
  --attribute-mapping="google.subject=assertion.sub"
```

### 8.3 Concurrency (RECOMMENDED)

Workflows using BYOS push SHOULD include concurrency group:

```yaml
concurrency:
  group: assay-evidence-${{ github.ref }}
  cancel-in-progress: false
```

---

## 9. Attestation Flow

### 9.1 Provenance Chain

```
┌─────────────────────────────────────────────────────────────┐
│                    Evidence Bundle                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐         │
│  │ Mandate Sig │  │ Bundle Hash │  │ Attestation │         │
│  │ (DSSE/Ed25519)│ │ (SHA256)    │  │ (GitHub)    │         │
│  └─────────────┘  └─────────────┘  └─────────────┘         │
│        │                │                │                  │
│        ▼                ▼                ▼                  │
│  User Authorization  Content ID     Build Provenance        │
└─────────────────────────────────────────────────────────────┘
```

### 9.2 Attestation Action Usage

```yaml
- name: Generate attestation
  id: attest
  uses: actions/attest-build-provenance@v3
  with:
    subject-path: ${{ steps.process.outputs.reports_dir }}/*.tar.gz
```

**Action Outputs:**

| Output | Description |
|--------|-------------|
| `attestation-id` | UUID of the attestation |
| `attestation-url` | GitHub UI link to view attestation |
| `bundle-path` | Local path to Sigstore bundle file |

**Mapping to action outputs:**

```yaml
- name: Export attestation outputs
  if: steps.attest.outcome == 'success'
  run: |
    echo "attestation_id=${{ steps.attest.outputs.attestation-id }}" >> $GITHUB_OUTPUT
    echo "attestation_url=${{ steps.attest.outputs.attestation-url }}" >> $GITHUB_OUTPUT
    echo "attestation_bundle_path=${{ steps.attest.outputs.bundle-path }}" >> $GITHUB_OUTPUT
```

### 9.3 Verification

```bash
gh attestation verify bundle.tar.gz --owner OWNER
```

### 9.4 SLSA Positioning (NORMATIVE)

Documentation MUST say "SLSA-aligned provenance" NOT "SLSA Level N".

**Rationale**: SLSA levels require specific builder hardening properties that cannot be guaranteed by attestations alone.

### 9.5 Attestation Availability

Artifact attestations have **plan and visibility constraints**:

| Repository Type | Attestation Available |
|-----------------|----------------------|
| Public repos | ✅ Yes (all plans) |
| Private repos (Enterprise Cloud) | ✅ Yes |
| Private repos (Team/Free) | ❌ No |
| Internal repos (Enterprise) | ✅ Yes |

**Implementation requirement (NORMATIVE)**:

When attestation is requested but not available:
1. Action MUST NOT fail (unless `fail_on` explicitly configured)
2. Action MUST set `attestation_id` and `attestation_url` to empty strings
3. Action MUST add Job Summary warning:

```markdown
> ⚠️ **Attestation skipped**: Artifact attestations require GitHub Enterprise Cloud
> for private repositories. [Learn more](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations)
```

**Detection**: Check for 403/404 response from attestation API or use `github.event.repository.visibility` + plan detection.

### 9.6 Subject Path Binding (NORMATIVE)

The attestation `subject-path` MUST reference the action's generated bundle(s):

```yaml
- uses: actions/attest-build-provenance@v3
  with:
    subject-path: ${{ steps.process.outputs.reports_dir }}/*.tar.gz
```

**Rationale**: Using a generic path like `evidence/*.tar.gz` may attest nothing if the action
outputs bundles elsewhere. Always bind to the actual output path.

---

## 10. Job Summary Format

### 10.1 Structure

```markdown
## Assay Evidence Report

**Status:** ✅ Passed | ❌ Failed

| Metric | Value |
|--------|-------|
| Bundles | N |
| Verified | N/N |
| Errors | N |
| Warnings | N |
| Code Scanning | ✅ Uploaded | ❌ Not uploaded |

## Compliance Pack Results

| Pack | Version | Score | Articles |
|------|---------|-------|----------|
| pack-name | 1.0.0 | 85% | 12(1), 12(2) |

> ⚠️ **Disclaimer**: [disclaimer text from SARIF]

## Attestation

| Field | Value |
|-------|-------|
| ID | uuid |
| URL | [View](url) |

---
[Documentation](link) | [Report Issue](link)
```

### 10.2 Conditional Sections

| Section | Condition |
|---------|-----------|
| Compliance Pack Results | `pack` input set |
| Attestation | `attest=true` and success |
| Disclaimer | Present in SARIF |

---

## 11. Error Handling

### 11.1 Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Success |
| 1 | Findings exceed threshold |
| 2 | Verification failed |
| 3 | Configuration error |
| 4 | OIDC authentication failed |
| 5 | Store push failed |

### 11.2 Write Operation Gating Strategy (NORMATIVE)

Write operations MUST be gated with `if:` conditionals, NOT masked with `continue-on-error`.

**Primary control**: Skip steps that cannot succeed (no permissions, wrong context).
**Secondary control**: `continue-on-error` only for truly optional UX features.

| Operation | Gating Strategy | `continue-on-error` |
|-----------|-----------------|---------------------|
| SARIF Upload | `if:` gate on same-repo context | `false` |
| PR Comment | `if:` gate on `pull_request` + same-repo | `true` (optional UX) |
| Baseline Write | `if:` gate on default branch push | `false` |
| BYOS Push | `if:` gate on default branch push | `false` |
| Attestation | `if:` gate on default branch push | `true` (infra flakiness) |
| Badge Update | `if:` gate on default branch push | `true` (optional UX) |

**SARIF Upload gating** (NORMATIVE):

```yaml
- name: Upload SARIF
  if: |
    inputs.sarif == 'true' &&
    github.event.pull_request.head.repo.full_name == github.repository
  uses: github/codeql-action/upload-sarif@...
```

> **Rationale**: Using `continue-on-error: true` for security-relevant operations (SARIF)
> can mask real regressions on same-repo PRs where permissions _are_ available.
> Gate the step instead; it simply won't run on fork PRs.

**Attestation failure handling**:

When attestation fails (permissions, plan limitations), the action MUST:
1. Set `attestation_id` output to empty string
2. Add warning to Job Summary: "Attestation skipped: [reason]"
3. NOT fail the overall workflow (unless `fail_on` is set to catch this)

---

## 12. Implementation Checklist

> **Status**: All Epics implemented as of v2.12.0. Contract tests verified in `.github/workflows/action-tests.yml`.

### Epic 1: Compliance Pack Support (P1) ✅

- [x] E1.1: Add `pack` input
- [x] E1.2: Pass `--pack` to `assay evidence lint`
- [x] E1.3: Extract pack metadata from SARIF
- [x] E1.4: Add `pack_applied`, `pack_score`, `pack_articles` outputs
- [x] E1.5: Job Summary with disclaimer (MANDATORY when present)
- [x] E1.6: Tests with `eu-ai-act-baseline` (`test-pack-lint`)

### Epic 2: BYOS Push + OIDC (P2) ✅

- [x] E2.1: Add `store`, `store_provider`, `store_role`, `store_region` inputs
- [x] E2.2: Add Azure inputs (`azure_client_id`, `azure_tenant_id`, `azure_subscription_id`)
- [x] E2.3: Store URL validation (fail-closed)
- [x] E2.4: Store role/identity validation (required inputs per provider)
- [x] E2.5: AWS OIDC configuration step
- [x] E2.6: GCP OIDC configuration step
- [x] E2.7: Azure OIDC configuration step
- [x] E2.8: Default branch guard (use `github.event.repository.default_branch`)
- [x] E2.9: Push step with `assay evidence push`
- [x] E2.10: Add `bundle_url` output
- [x] E2.11: Document IAM setup per provider (incl. Azure federated credentials)
- [x] E2.12: OIDC auto-detection test (`test-oidc-detection`)

### Epic 3: Artifact Attestation (P3) ✅

- [x] E3.1: Add `attest` input
- [x] E3.2: Integrate `actions/attest-build-provenance@v3`
- [x] E3.3: Default branch guard (use `github.event.repository.default_branch`)
- [x] E3.4: Add `attestation_id`, `attestation_url`, `attestation_bundle_path` outputs
- [x] E3.5: Bind `subject-path` to `steps.process.outputs.reports_dir`
- [x] E3.6: Job Summary attestation section
- [x] E3.7: Attestation availability detection (plan/visibility check)
- [x] E3.8: Job Summary warning when attestation unavailable/skipped
- [x] E3.9: Document permission requirements (`attestations: write`, `id-token: write`)
- [x] E3.10: Document verification command (`gh attestation verify`)
- [x] E3.11: Attestation gating test (`test-attestation-gating`)

### Epic 4: Coverage Badge (P4) ✅

- [x] E4.1: Add `badge_gist` input
- [x] E4.2: Default branch guard (use `github.event.repository.default_branch`)
- [x] E4.3: Integrate `schneegans/dynamic-badges-action`
- [x] E4.4: Implement coverage calculation per §4.4 (tools_with_decisions / tools_observed)
- [x] E4.5: Handle multi-bundle aggregation (union, dedupe)
- [x] E4.6: Add `coverage_percent` output
- [x] E4.7: Document GIST_TOKEN requirements (fine-grained PAT, `gist` scope)
- [x] E4.8: Coverage calculation test (`test-coverage-formula`)

### Epic 5: Documentation & Release ✅

- [x] E5.1: Update README with all new inputs/outputs
- [x] E5.2: Add OIDC setup guides (AWS, GCP, Azure)
- [x] E5.3: Add compliance pack usage examples
- [x] E5.4: Add attestation verification guide
- [x] E5.5: Update Marketplace listing (pending)
- [x] E5.6: Release notes (CHANGELOG.md v2.12.0)

---

## 13. References

### Normative

- [ADR-018: GitHub Action v2.1](./ADR-018-GitHub-Action-v2.1.md)
- [SPEC-Pack-Engine-v1](./SPEC-Pack-Engine-v1.md)
- [SARIF 2.1.0 Specification](https://docs.oasis-open.org/sarif/sarif/v2.1.0/sarif-v2.1.0.html)

### Informative

- [GitHub Artifact Attestations](https://docs.github.com/en/actions/security-for-github-actions/using-artifact-attestations)
- [GitHub OIDC with AWS](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-amazon-web-services)
- [GitHub OIDC with GCP](https://docs.github.com/en/actions/deployment/security-hardening-your-deployments/configuring-openid-connect-in-google-cloud-platform)
- [EU AI Act (Regulation 2024/1689)](https://eur-lex.europa.eu/eli/reg/2024/1689/oj)

---

## Appendix A: Action Pinning Reference

All third-party actions MUST be pinned to commit SHA.

**Maintenance requirement (NORMATIVE)**: Release tooling MUST periodically verify that each
SHA corresponds to the intended major/minor tag. Recommended: quarterly audit or on each release.

```yaml
# Verified Jan 2026
actions/cache@0c907a75c2c80ebcb7f088228285e798b750cf8f                    # v4.2.1
actions/upload-artifact@65c4c4a1ddee5b72f698fdd19549f0f0fb45cf08          # v4.6.0
github/codeql-action/upload-sarif@b20883b0cd1f46c72ae0ba6d1090936928f9fa30 # v4.32.0
actions/attest-build-provenance@1c608d11d69870c2092266b3f9a6f3abbf17002c  # v3.0.0
aws-actions/configure-aws-credentials@e3dd6a429d7300a6a4c196c26e071d42e0343502 # v4.0.2
google-github-actions/auth@6fc4af4b145ae7821d527454aa9bd537d1f2dc5f       # v2.1.7
azure/login@a65d910e8af852a8061c627c456678983e180302                      # v2.2.0
schneegans/dynamic-badges-action@e9a478b16159b4d31420099ba146cdc50f134483 # v1.7.0
peter-evans/find-comment@3eae4d37986fb5a8592848f6a574fdf654e61f9e         # v3.1.0
peter-evans/create-or-update-comment@71345be0265236311c031f5c7866368bd1ebb043 # v4.0.0
```

**Verification command:**

```bash
# Verify SHA matches expected tag
git ls-remote --tags https://github.com/actions/cache.git | grep v4.2.1
```

---

## Appendix B: Minimum IAM Policies

### B.1 AWS S3 Push

**Minimum required:**

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": [
      "s3:PutObject"
    ],
    "Resource": "arn:aws:s3:::BUCKET/evidence/*"
  }]
}
```

**Optional (for ACL-enabled buckets only):**

```json
{
  "Action": ["s3:PutObjectAcl"],
  "Resource": "arn:aws:s3:::BUCKET/evidence/*"
}
```

> **Note**: Most modern S3 configurations use "Bucket owner enforced" object ownership,
> which disables ACLs. Only add `s3:PutObjectAcl` if your bucket explicitly requires ACL management.
> Prefer SSE (server-side encryption) and bucket policies over object ACLs.

### B.2 GCP GCS Push

```yaml
roles/storage.objectCreator on gs://BUCKET
```

### B.3 Azure Blob Push

```
Storage Blob Data Contributor on container
```

Required Azure OIDC federated credential configuration:
- Issuer: `https://token.actions.githubusercontent.com`
- Subject: `repo:ORG/REPO:ref:refs/heads/main`
- Audience: `api://AzureADTokenExchange`
