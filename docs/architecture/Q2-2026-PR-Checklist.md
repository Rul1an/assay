# Q2 2026 PR Checklist

This document defines the PR sequence for Q2 2026 (Commercial Alpha) with acceptance criteria per PR.

## Overview

```
Week 1-2: PR-A, PR-B (Core CLI + Ingest API)
Week 3-4: PR-C, PR-D (WORM + Legal Hold, Signing)
Week 5-6: PR-E, PR-F (Sigstore, EU AI Act Pack)
Week 7-8: PR-G (Production Hardening)
```

---

## PR-A: Evidence Store CLI Commands (Open Core)

**Branch:** `feature/evidence-store-cli`

**Scope:** Add `assay evidence push/pull/list` commands to CLI (open core). Backend is stubbed/configurable.

### Files to Change
- `crates/assay-cli/src/cli/commands/evidence/mod.rs`
- `crates/assay-cli/src/cli/commands/evidence/push.rs` (new)
- `crates/assay-cli/src/cli/commands/evidence/pull.rs` (new)
- `crates/assay-cli/src/cli/commands/evidence/list.rs` (new)
- `crates/assay-cli/src/cli/args.rs`

### Acceptance Criteria
- [ ] `assay evidence push bundle.tar.gz --store <url>` sends bundle to configured endpoint
- [ ] `assay evidence pull --bundle-id <id> --out <path>` downloads bundle
- [ ] `assay evidence list --run-id <id>` lists bundles (paginated)
- [ ] Environment variables: `ASSAY_STORE_URL`, `ASSAY_STORE_API_KEY`
- [ ] Config in `assay.yaml`: `evidence_store.url`, `evidence_store.auto_push`
- [ ] Graceful error handling when store is unavailable
- [ ] Unit tests for argument parsing
- [ ] Integration test with mock server

### Definition of Done
- [ ] Code reviewed
- [ ] Tests pass
- [ ] Documentation updated (`docs/reference/cli/evidence.md`)

---

## PR-B: Evidence Store Ingest API

**Branch:** `feature/evidence-store-api`

**Scope:** Implement the core ingest API (POST /v1/bundles, GET /v1/bundles/{id}).

### Files to Create (New Service)
- `services/evidence-store/src/main.rs`
- `services/evidence-store/src/api/mod.rs`
- `services/evidence-store/src/api/bundles.rs`
- `services/evidence-store/src/storage/s3.rs`
- `services/evidence-store/src/storage/dynamodb.rs`
- `services/evidence-store/src/auth/api_key.rs`
- `services/evidence-store/Cargo.toml`
- `services/evidence-store/Dockerfile`

### Acceptance Criteria
- [ ] `POST /v1/bundles` accepts gzip bundle, returns 201 with metadata
- [ ] Bundle verification runs before storage (`verify_bundle_with_limits`)
- [ ] `GET /v1/bundles/{bundle_id}` returns bundle metadata
- [ ] `GET /v1/bundles/{bundle_id}/download` returns bundle content
- [ ] API key authentication (`Authorization: Bearer <key>`)
- [ ] Rate limiting (100 req/min default)
- [ ] Idempotent uploads (409 for duplicate `bundle_id`)
- [ ] DynamoDB metadata storage
- [ ] S3 bundle storage (without Object Lock for now)
- [ ] Health check endpoint (`GET /health`)
- [ ] OpenAPI spec (`openapi.yaml`)

### Definition of Done
- [ ] Integration tests with LocalStack (S3 + DynamoDB)
- [ ] Load test: 100 concurrent uploads
- [ ] Deployed to staging environment

---

## PR-C: WORM Storage + Legal Hold

**Branch:** `feature/worm-storage`

**Scope:** Enable S3 Object Lock and implement legal hold API.

### Files to Change
- `services/evidence-store/src/storage/s3.rs`
- `services/evidence-store/src/api/bundles.rs`
- `services/evidence-store/src/api/legal_hold.rs` (new)
- `infrastructure/terraform/s3.tf` (or CloudFormation)

### Acceptance Criteria
- [ ] S3 bucket has Object Lock enabled (Compliance mode)
- [ ] Default retention: 90 days
- [ ] `POST /v1/bundles` sets `ObjectLockRetainUntilDate`
- [ ] `POST /v1/bundles/{id}/legal-hold` enables/disables legal hold
- [ ] `GET /v1/bundles/{id}` includes `legal_hold` status
- [ ] `DELETE /v1/bundles/{id}` returns 403 during retention/legal hold
- [ ] Retention tier selection via header (`X-Assay-Retention-Tier`)
- [ ] CloudTrail logging for all Object Lock operations

### Definition of Done
- [ ] Compliance mode verified (cannot delete during retention)
- [ ] Legal hold verified (cannot delete with hold active)
- [ ] Documentation: compliance procedures for SEC 17a-4

---

## PR-D: Tool Signing (ed25519 Local)

**Branch:** `feature/tool-signing-local`

**Scope:** Implement local ed25519 signing and verification for `x-assay-sig`.

### Files to Change
- `crates/assay-core/src/mcp/signing.rs` (new)
- `crates/assay-core/src/mcp/identity.rs`
- `crates/assay-cli/src/cli/commands/tool/mod.rs` (new)
- `crates/assay-cli/src/cli/commands/tool/sign.rs` (new)
- `crates/assay-cli/src/cli/commands/tool/verify.rs` (new)

### Acceptance Criteria
- [ ] `assay tool sign --key <private.pem> tool.json` produces signed output
- [ ] `assay tool verify tool.json` verifies signature
- [ ] `x-assay-sig` field format matches ADR-011 spec
- [ ] Signature covers JCS-canonical tool definition
- [ ] `VerifyError::ProducerUntrusted` for untrusted identities
- [ ] `VerifyError::SignatureInvalid` for bad signatures
- [ ] Trust policy in `assay.yaml` (`tool_verification.trust_anchors`)
- [ ] `--require-producer-trust` flag for `assay evidence verify`

### Definition of Done
- [ ] Unit tests for signing/verification
- [ ] Integration test: sign → modify → verify fails
- [ ] Documentation: key generation guide

---

## PR-E: Sigstore Keyless Integration

**Branch:** `feature/sigstore-keyless`

**Scope:** Add Fulcio certificate issuance and Rekor transparency logging.

### Files to Change
- `crates/assay-core/src/mcp/sigstore.rs` (new)
- `crates/assay-core/src/mcp/rekor.rs` (new)
- `crates/assay-core/src/mcp/fulcio.rs` (new)
- `crates/assay-cli/src/cli/commands/tool/sign.rs`
- `crates/assay-cli/src/cli/commands/tool/verify.rs`

### Dependencies
- `sigstore` crate (or direct API calls)
- TUF client for root distribution

### Acceptance Criteria
- [ ] `assay tool sign --keyless tool.json` triggers OIDC flow
- [ ] Fulcio certificate obtained and embedded in `x-assay-sig`
- [ ] Signature recorded in Rekor, `rekor_entry` UUID stored
- [ ] `assay tool verify --rekor-required` verifies inclusion proof
- [ ] TUF root initialized on first run
- [ ] Offline bundle support (`x-assay-sig.rekor_bundle`)
- [ ] GitHub Actions identity supported
- [ ] Google/Microsoft OIDC supported

### Definition of Done
- [ ] E2E test: keyless sign → verify with Rekor
- [ ] Documentation: OIDC provider setup
- [ ] CI integration example (GitHub Actions)

---

## PR-F: EU AI Act Compliance Pack

**Branch:** `feature/eu-ai-act-pack`

**Scope:** Implement pack system and EU AI Act baseline pack.

### Files to Create
- `crates/assay-evidence/src/lint/packs/mod.rs` (new)
- `crates/assay-evidence/src/lint/packs/loader.rs` (new)
- `crates/assay-evidence/src/lint/packs/engine.rs` (new)
- `packs/eu-ai-act.yaml` (new)
- `packs/schema.json` (pack definition schema)

### Files to Change
- `crates/assay-cli/src/cli/commands/evidence/lint.rs`
- `crates/assay-evidence/src/lint/sarif.rs`

### Acceptance Criteria
- [ ] `assay evidence lint --pack eu-ai-act@1.0.0` loads and runs pack
- [ ] Pack rules produce findings with `article_ref`
- [ ] SARIF output includes pack metadata in `tool.extensions`
- [ ] Disclaimer included in all outputs
- [ ] Pack composition: `--pack a,b` merges rules
- [ ] Pack version resolution (semver)
- [ ] Custom pack from file: `--pack ./my-pack.yaml`
- [ ] Built-in packs: `eu-ai-act@1.0.0`

### Definition of Done
- [ ] All EU12-* rules implemented per ADR-013
- [ ] Integration test with sample bundle
- [ ] Documentation: pack authoring guide

---

## PR-G: Production Hardening

**Branch:** `feature/production-hardening`

**Scope:** Security hardening, monitoring, and production readiness.

### Files to Change
- `services/evidence-store/src/auth/`
- `services/evidence-store/src/middleware/`
- `infrastructure/terraform/` or `infrastructure/cloudformation/`

### Acceptance Criteria
- [ ] Signed upload tokens (`POST /v1/upload-tokens`)
- [ ] Per-tenant KMS key separation
- [ ] CloudWatch dashboards (latency, errors, throughput)
- [ ] Alerting on 5xx errors, high latency
- [ ] Load test: 1000 req/s sustained
- [ ] Penetration test: no critical findings
- [ ] SOC 2 control documentation

### Definition of Done
- [ ] Security review completed
- [ ] Runbook for incident response
- [ ] GA release checklist complete

---

## Dependency Graph

```
PR-A (CLI) ──────────────────────────┐
                                     │
PR-B (Ingest API) ───────────────────┼──→ PR-C (WORM)
                                     │
PR-D (Local Signing) ────────────────┼──→ PR-E (Sigstore)
                                     │
PR-F (EU AI Act Pack) ───────────────┘
                                     │
                                     ▼
                              PR-G (Hardening)
```

**Parallel tracks:**
- Track 1: PR-A → PR-B → PR-C (Evidence Store)
- Track 2: PR-D → PR-E (Signing)
- Track 3: PR-F (Compliance Pack)

All converge at PR-G (Production Hardening).

---

## Release Milestones

| Milestone | PRs | Target Date |
|-----------|-----|-------------|
| **Alpha** | PR-A, PR-B | Week 2 |
| **Beta** | PR-C, PR-D, PR-F | Week 4 |
| **RC** | PR-E, PR-G | Week 6 |
| **GA** | All | Week 8 |
