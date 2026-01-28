# ADR-015: BYOS (Bring Your Own Storage) Strategy

## Status

Accepted (January 2026)

## Context

The original roadmap (ADR-009, ADR-010) planned a managed Evidence Store with:
- AWS S3 Object Lock for WORM compliance
- REST API for multi-tenant ingest
- Managed infrastructure (Lambda, DynamoDB, API Gateway)

After analysis of 2025-2026 market conditions and startup economics, we identified several issues:

### Problems with Managed-First Approach

1. **Premature Infrastructure**: Building cloud infrastructure before product-market fit
2. **Commoditized Storage**: WORM storage is a commodity (Backblaze, Wasabi, R2 all offer it)
3. **User Needs**: Enterprise users already have compliant storage; they need **tools**, not hosting
4. **Cost**: AWS infrastructure costs $50-200+/month even at minimal scale
5. **Compliance Burden**: SEC 17a-4 certification requires ongoing audits and legal work

### Market Research (January 2026)

| Provider | Storage/GB | Egress | SEC 17a-4 | Free Tier |
|----------|------------|--------|-----------|-----------|
| AWS S3 | $0.023 | $0.09/GB | ✅ Cohasset | Limited |
| Backblaze B2 | $0.006 | $0.01/GB | ✅ Object Lock | 10GB |
| Wasabi | $0.0049 | $0.00 | ✅ Cohasset | None |
| Cloudflare R2 | $0.015 | $0.00 | ⚠️ No cert | 10GB |
| MinIO | Self-host | N/A | ✅ Cohasset | Free |

**Key Insight**: Users with compliance requirements already have storage infrastructure. They need CLI tools that work with their existing setup.

### Industry Trends (2025-2026)

1. **Library-First > SaaS-First**: RivetKit pattern - portable libraries over external dependencies
2. **BYOS Adoption**: Tools like Litestream, Chainloop, Retraced support self-hosted deployment
3. **EU AI Act Deadline**: August 2026 - organizations need compliance tools NOW, not hosting
4. **70% Gap**: Most organizations have gaps in audit trail implementation (SparkCo 2025 report)

## Decision

We will implement a **BYOS-first (Bring Your Own Storage)** strategy:

1. **CLI commands work with any S3-compatible storage**
2. **No managed infrastructure in Phase 1**
3. **User configures their own WORM-compliant bucket**
4. **Managed hosting deferred until proven demand**

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    User's Environment                            │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      assay CLI                                   │
│                                                                  │
│  assay evidence push bundle.tar.gz                              │
│  assay evidence pull --bundle-id sha256:...                     │
│  assay evidence list --run-id run_123                           │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│              Generic S3 Client (object_store crate)              │
│                                                                  │
│  Supports: AWS S3, Backblaze B2, Wasabi, R2, MinIO, Tigris     │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ User's AWS S3   │ │ User's B2       │ │ User's MinIO    │
│ (Object Lock)   │ │ (Object Lock)   │ │ (Self-hosted)   │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

### CLI Commands (Open Core)

```bash
# Configuration (environment variables or assay.yaml)
export ASSAY_STORE_ENDPOINT=s3.us-west-002.backblazeb2.com
export ASSAY_STORE_BUCKET=my-evidence-bucket
export ASSAY_STORE_ACCESS_KEY=...
export ASSAY_STORE_SECRET_KEY=...

# Push bundle to user's storage
assay evidence push bundle.tar.gz
assay evidence push bundle.tar.gz --run-id run_123

# Pull bundle from user's storage
assay evidence pull --bundle-id sha256:ade9c15d... --out ./bundle.tar.gz
assay evidence pull --run-id run_123 --out ./bundles/

# List bundles
assay evidence list
assay evidence list --run-id run_123
assay evidence list --after 2026-01-01

# Check storage status
assay evidence store-status
```

### Configuration

```yaml
# assay.yaml
evidence_store:
  # S3-compatible endpoint (required)
  endpoint: s3.us-west-002.backblazeb2.com
  bucket: my-evidence-bucket

  # Credentials (can also be environment variables)
  # access_key: from ASSAY_STORE_ACCESS_KEY
  # secret_key: from ASSAY_STORE_SECRET_KEY

  # Optional settings
  region: us-west-002
  path_prefix: assay/bundles/  # Organize within bucket

  # Behavior
  auto_push: false  # Push after every export
  verify_on_push: true  # Verify bundle before upload
```

### Object Key Schema (Simplified)

```
{prefix}/bundles/{bundle_id}.tar.gz        # Primary (content-addressed, immutable)
{prefix}/runs/{run_id}/{bundle_id}.ref     # Run index (small reference file)

Examples:
assay/evidence/bundles/sha256:ade9c15d....tar.gz
assay/evidence/runs/run_001/sha256:ade9c15d....ref
```

**Design rationale:**
- **O(1) operations**: `pull --bundle-id` = direct key lookup; `list --run-id` = prefix list
- **No date folders**: Lifecycle policies use object metadata/tags, not path structure
- **Content-addressed**: `bundle_id` (SHA-256 of run_root) is the single source of truth
- **Immutability**: Enforced via conditional writes (`PutMode::Create` / If-None-Match)

### Environment Variables

| Variable | Description | Required |
|----------|-------------|----------|
| `ASSAY_STORE_URL` | Store URL (`s3://bucket/prefix`) | Yes |
| `AWS_ACCESS_KEY_ID` | AWS/S3-compatible credentials | Yes* |
| `AWS_SECRET_ACCESS_KEY` | AWS/S3-compatible credentials | Yes* |
| `AWS_REGION` | Default region | No |
| `ASSAY_STORE_REGION` | Override region (highest precedence) | No |
| `ASSAY_STORE_ALLOW_HTTP` | Allow HTTP for dev (MinIO, LocalStack) | No |
| `ASSAY_STORE_PATH_STYLE` | Use path-style URLs for S3-compat | No |

\* Or use IAM roles/instance profiles

### Verification Flow

```rust
async fn push_bundle(path: &Path, store: &BundleStore) -> Result<PushResult> {
    // 1. Verify bundle integrity locally
    let result = verify_bundle(File::open(path)?, VerifyLimits::default())?;
    let manifest = result.manifest;

    // 2. Upload with conditional write (immutability)
    // Uses PutMode::Create (If-None-Match: "*")
    match store.put_bundle(&manifest.bundle_id, bytes).await {
        Ok(()) => {
            // 3. Link to run_id for list queries
            if let Some(run_id) = &manifest.run_id {
                store.link_run_bundle(run_id, &manifest.bundle_id).await?;
            }
            Ok(PushResult::Uploaded { bundle_id: manifest.bundle_id })
        }
        Err(StoreError::AlreadyExists { .. }) => {
            // Idempotent: same bundle_id = same bytes
            Ok(PushResult::AlreadyExists { bundle_id: manifest.bundle_id })
        }
        Err(e) => Err(e.into()),
    }
}
```

**Immutability guarantees:**

| Backend | Conditional Write | Guarantee |
|---------|-------------------|-----------|
| AWS S3 | ✅ `PutMode::Create` | Strong |
| MinIO (recent) | ✅ | Strong |
| R2/B2/Wasabi | ⚠️ Varies | Check docs |
| file:// | ✅ | Strong |
| memory:// | ✅ | Strong |

If conditional writes fail with "not supported", Assay falls back to check-then-put with a warning ("immutability not guaranteed").

## Phases

### Phase 1: BYOS CLI (Q2 2026)

- [x] Generic S3 client using `object_store` crate
- [ ] `assay evidence push` command
- [ ] `assay evidence pull` command
- [ ] `assay evidence list` command
- [ ] `assay evidence store-status` command
- [ ] Configuration via env vars and assay.yaml
- [ ] Documentation for configuring AWS S3, Backblaze B2, Wasabi, R2, MinIO

### Phase 2: GitHub Action Integration (Q2 2026)

- [ ] Action input for store configuration
- [ ] Auto-push after verify/lint
- [ ] Pull baseline from store for comparison

### Phase 3: Managed Store (Q3+ 2026, IF demand)

Only proceed if:
1. Users explicitly request managed hosting
2. Revenue model supports infrastructure costs
3. Product-market fit is validated

Then:
- Cloudflare Workers + R2 (non-SEC-compliant tier)
- Backblaze B2 Object Lock proxy (SEC-compliant tier)
- Pricing: pass-through storage + margin

## Alternatives Considered

### 1. Managed-First (Original Plan)

**Pros:**
- Single integration point
- Controlled compliance environment
- Potential revenue source

**Cons:**
- High upfront infrastructure cost
- Commoditized offering (no differentiation)
- Delays value-add features (signing, compliance packs)
- Users with compliance needs already have storage

**Decision:** Rejected for Phase 1. Reconsider in Phase 3.

### 2. Proprietary Protocol

**Pros:**
- Lock-in potential
- Custom optimizations

**Cons:**
- Higher adoption friction
- No ecosystem benefits
- Maintenance burden

**Decision:** Rejected. S3 API is the standard.

### 3. Git-Based Storage (git-lfs pattern)

**Pros:**
- Familiar to developers
- Built-in versioning

**Cons:**
- Not designed for compliance/WORM
- Performance issues at scale
- No native Object Lock

**Decision:** Rejected. S3 is better fit for compliance use cases.

## Consequences

### Positive

- **$0 infrastructure cost** for Assay project
- **Faster time-to-value**: Focus on CLI features, not cloud ops
- **User choice**: Works with existing storage infrastructure
- **Compliance flexibility**: User controls their WORM configuration
- **Lower adoption friction**: No API keys, no account creation

### Negative

- **No recurring storage revenue** (initially)
- **User responsibility** for WORM configuration
- **Support complexity**: Multiple storage providers

### Neutral

- S3 API compatibility is well-established
- Object Lock semantics are consistent across providers
- Migration path to managed store is straightforward

## Security Considerations

### Credential Management

- Credentials via environment variables (not in config files)
- Support for IAM roles (AWS), Application Keys (B2), etc.
- Never log credentials

### Bundle Integrity

- Always verify bundle before push
- Store `x-assay-bundle-id` metadata for verification
- Support checksum validation on pull

### WORM Responsibility

User is responsible for configuring Object Lock on their bucket:
- Document recommended configurations per provider
- Warn if bucket doesn't have Object Lock enabled (best effort detection)
- Provide verification commands to check compliance setup

## References

- [AWS S3 Object Lock](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock.html)
- [Backblaze B2 Object Lock](https://www.backblaze.com/docs/cloud-storage-object-lock)
- [Wasabi Object Lock](https://wasabi.com/cloud-object-storage/s3-object-lock)
- [Cloudflare R2 Bucket Locks](https://developers.cloudflare.com/r2/buckets/bucket-locks/)
- [MinIO Object Locking](https://min.io/docs/minio/linux/administration/object-management/object-retention.html)
- [object_store crate](https://docs.rs/object_store/latest/object_store/)
- [ADR-009: WORM Storage](./ADR-009-WORM-Storage.md) (superseded for Phase 1)
- [ADR-010: Evidence Store API](./ADR-010-Evidence-Store-API.md) (deferred to Phase 3)
