# SPEC-GitHub-Action-v2.1

**Version:** 1.0.0
**Status:** Draft
**Date:** 2026-01-29
**ADR:** [ADR-018](./ADR-018-GitHub-Action-v2.1.md)

## Abstract

This specification defines the GitHub Action v2.1 interface, behavior, and implementation requirements. It covers compliance pack integration, BYOS push with OIDC authentication, artifact attestation, and coverage badge generation.

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
| `store_role` | string | `''` | Conditional | IAM role/identity for OIDC |
| `store_region` | string | `us-east-1` | No | AWS region (AWS only) |
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

**3.3.4 Store Role Requirement**

| Provider | `store_role` Required | Format |
|----------|----------------------|--------|
| `aws` | Yes | `arn:aws:iam::ACCOUNT:role/ROLE` |
| `gcp` | Yes | `projects/PROJECT/locations/global/workloadIdentityPools/POOL/providers/PROVIDER` |
| `azure` | No (uses azure/* inputs) | N/A |

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
| `pack_applied` | string | `pack` set | Comma-separated pack IDs |
| `pack_score` | int | `pack` set | Compliance score (0-100) |
| `pack_articles` | string | `pack` set | Covered articles |
| `bundle_url` | string | `store` set | Pushed bundle URL |
| `attestation_id` | string | `attest=true` | Attestation UUID |
| `attestation_url` | string | `attest=true` | Attestation view URL |
| `coverage_percent` | int | Always | Coverage percentage |

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
| OIDC (any) | `id-token: write` |

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
- uses: actions/attest-build-provenance@v3
  with:
    subject-path: evidence/*.tar.gz
```

**Outputs:**
- `attestation-id`: UUID
- `attestation-url`: GitHub UI link

### 9.3 Verification

```bash
gh attestation verify bundle.tar.gz --owner OWNER
```

### 9.4 SLSA Positioning (NORMATIVE)

Documentation MUST say "SLSA-aligned provenance" NOT "SLSA Level N".

**Rationale**: SLSA levels require specific builder hardening properties that cannot be guaranteed by attestations alone.

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

### 11.2 Continue-on-Error Operations

| Operation | `continue-on-error` | Rationale |
|-----------|---------------------|-----------|
| SARIF Upload | `true` | Fork PRs lack permission |
| PR Comment | `true` | Optional feature |
| Attestation | `true` | Should not block on infra issues |
| Badge Update | `true` | Non-critical |
| BYOS Push | `false` | Data loss if silent failure |

---

## 12. Implementation Checklist

### Epic 1: Compliance Pack Support (P1)

- [ ] E1.1: Add `pack` input
- [ ] E1.2: Pass `--pack` to `assay evidence lint`
- [ ] E1.3: Extract pack metadata from SARIF
- [ ] E1.4: Add `pack_applied`, `pack_score`, `pack_articles` outputs
- [ ] E1.5: Job Summary with disclaimer (MANDATORY when present)
- [ ] E1.6: Tests with `eu-ai-act-baseline`

### Epic 2: BYOS Push + OIDC (P2)

- [ ] E2.1: Add `store`, `store_provider`, `store_role`, `store_region` inputs
- [ ] E2.2: Store URL validation (fail-closed)
- [ ] E2.3: Store role validation (required for aws/gcp)
- [ ] E2.4: AWS OIDC configuration step
- [ ] E2.5: GCP OIDC configuration step
- [ ] E2.6: Azure OIDC configuration step (optional)
- [ ] E2.7: Default branch guard
- [ ] E2.8: Push step with `assay evidence push`
- [ ] E2.9: Add `bundle_url` output
- [ ] E2.10: Document IAM setup per provider
- [ ] E2.11: E2E test with test bucket

### Epic 3: Artifact Attestation (P3)

- [ ] E3.1: Add `attest` input
- [ ] E3.2: Integrate `actions/attest-build-provenance@v3`
- [ ] E3.3: Default branch guard
- [ ] E3.4: Add `attestation_id`, `attestation_url` outputs
- [ ] E3.5: Job Summary attestation section
- [ ] E3.6: Document permission requirements
- [ ] E3.7: Document verification command
- [ ] E3.8: Test attestation generation

### Epic 4: Coverage Badge (P4)

- [ ] E4.1: Add `badge_gist` input
- [ ] E4.2: Default branch guard
- [ ] E4.3: Integrate `schneegans/dynamic-badges-action`
- [ ] E4.4: Compute `coverage_percent` output
- [ ] E4.5: Document GIST_TOKEN requirements
- [ ] E4.6: Test badge update

### Epic 5: Documentation & Release

- [ ] E5.1: Update README with all new inputs/outputs
- [ ] E5.2: Add OIDC setup guides (AWS, GCP, Azure)
- [ ] E5.3: Add compliance pack usage examples
- [ ] E5.4: Add attestation verification guide
- [ ] E5.5: Update Marketplace listing
- [ ] E5.6: Release notes

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

All third-party actions MUST be pinned to commit SHA:

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

---

## Appendix B: Minimum IAM Policies

### B.1 AWS S3 Push

```json
{
  "Version": "2012-10-17",
  "Statement": [{
    "Effect": "Allow",
    "Action": [
      "s3:PutObject",
      "s3:PutObjectAcl"
    ],
    "Resource": "arn:aws:s3:::BUCKET/evidence/*"
  }]
}
```

### B.2 GCP GCS Push

```yaml
roles/storage.objectCreator on gs://BUCKET
```

### B.3 Azure Blob Push

```
Storage Blob Data Contributor on container
```
