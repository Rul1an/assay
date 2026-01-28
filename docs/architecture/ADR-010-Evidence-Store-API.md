# ADR-010: Evidence Store Ingest API

## Status

Proposed (January 2026)

## Context

The Evidence Store MVP requires a REST API for:
1. Ingesting evidence bundles from `assay evidence export`
2. Querying bundles by `run_id`, `bundle_id`, `tenant_id`
3. Supporting multi-tenant SaaS with proper isolation

Key constraints:
- Bundles are already CloudEvents-compliant (see ADR-006)
- Content-addressed IDs (sha256) are computed client-side
- WORM storage backend (see ADR-009)
- Must scale to thousands of tenants

## Decision

We will implement a **CloudEvents-native REST API** with object key partitioning for multi-tenancy.

### API Design

#### Ingest Endpoint

```http
POST /v1/bundles
Authorization: Bearer {api_key}
Content-Type: application/gzip
X-Assay-Run-Id: {run_id}
X-Assay-Tenant-Id: {tenant_id}  # Derived from API key if omitted

{binary bundle content}
```

**Response (201 Created):**
```json
{
  "bundle_id": "sha256:ade9c15dbdb1cbfa696e8c65cc0b5fba...",
  "run_id": "run_baseline_001",
  "tenant_id": "tenant_abc123",
  "ingested_at": "2026-01-28T12:00:00Z",
  "retention_expires_at": "2026-04-28T12:00:00Z",
  "storage_bytes": 1078,
  "verified": true,
  "links": {
    "self": "/v1/bundles/sha256:ade9c15dbdb1cbfa696e8c65cc0b5fba",
    "download": "/v1/bundles/sha256:ade9c15dbdb1cbfa696e8c65cc0b5fba/download"
  }
}
```

**Error Responses:**
- `400 Bad Request`: Invalid bundle format, verification failed
- `401 Unauthorized`: Invalid or missing API key
- `409 Conflict`: Bundle with same `bundle_id` already exists (idempotent - return existing)
- `413 Payload Too Large`: Bundle exceeds size limit
- `429 Too Many Requests`: Rate limit exceeded

#### Query Endpoints

```http
# Get bundle metadata
GET /v1/bundles/{bundle_id}

# Download bundle
GET /v1/bundles/{bundle_id}/download

# List bundles by run
GET /v1/runs/{run_id}/bundles

# List bundles for tenant
GET /v1/bundles?run_id={run_id}&limit=100&cursor={cursor}

# Search bundles
POST /v1/bundles/search
{
  "filters": {
    "run_id": "run_*",
    "ingested_after": "2026-01-01T00:00:00Z",
    "event_types": ["assay.fs.access", "assay.net.connect"]
  },
  "limit": 100
}
```

#### Legal Hold Endpoint

```http
POST /v1/bundles/{bundle_id}/legal-hold
Authorization: Bearer {api_key}
Content-Type: application/json

{
  "enabled": true,
  "reason": "Investigation case #12345",
  "requested_by": "legal@example.com",
  "case_id": "CASE-2026-001"
}
```

Response:
```json
{
  "bundle_id": "sha256:ade9c15d...",
  "legal_hold": {
    "enabled": true,
    "reason": "Investigation case #12345",
    "requested_by": "legal@example.com",
    "case_id": "CASE-2026-001",
    "applied_at": "2026-01-28T12:00:00Z"
  }
}
```

### CLI Commands (Open Core)

The CLI provides open-core commands that work with the paid backend:

```bash
# Upload bundle to Evidence Store
assay evidence push bundle.tar.gz --store https://store.assay.dev
assay evidence push bundle.tar.gz --store $ASSAY_STORE_URL

# Download bundle from Evidence Store
assay evidence pull --bundle-id sha256:ade9c15d... --out ./bundle.tar.gz
assay evidence pull --run-id run_123 --out ./bundles/

# List bundles
assay evidence list --run-id run_123
assay evidence list --after 2026-01-01

# Check store status
assay evidence store-status
```

**Environment Variables:**
```bash
ASSAY_STORE_URL=https://store.assay.dev
ASSAY_STORE_API_KEY=assay_live_abc123...
```

**Configuration in assay.yaml:**
```yaml
evidence_store:
  url: https://store.assay.dev
  # API key from environment or config
  auto_push: false  # Set true to push after every export
```

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                          API Gateway                             │
│                    (Rate Limiting, Auth)                         │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      Lambda / Container                          │
│                                                                  │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────────┐ │
│  │ Auth Layer  │→ │ Verify      │→ │ Store (S3 + DynamoDB)   │ │
│  │ (API Key)   │  │ Bundle      │  │                         │ │
│  └─────────────┘  └─────────────┘  └─────────────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
                              │
              ┌───────────────┼───────────────┐
              ▼               ▼               ▼
┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐
│ S3 (Bundles)    │ │ DynamoDB        │ │ CloudWatch      │
│ Object Lock     │ │ (Metadata)      │ │ (Metrics/Logs)  │
│ WORM Storage    │ │ GSI: tenant_id  │ │                 │
└─────────────────┘ └─────────────────┘ └─────────────────┘
```

### Multi-Tenant Data Model

#### S3 Object Key Schema
```
/{tenant_id}/bundles/{year}/{month}/{run_id}/{bundle_id}.tar.gz

Example:
/tenant_abc123/bundles/2026/01/run_baseline_001/sha256:ade9c15d....tar.gz
```

**Rationale:** Object key partitioning scales better than bucket-per-tenant (AWS recommends this for >100 tenants).

#### DynamoDB Schema

**Table: `assay-evidence-bundles`**

| Attribute | Type | Description |
|-----------|------|-------------|
| `pk` | String | `TENANT#{tenant_id}` |
| `sk` | String | `BUNDLE#{bundle_id}` |
| `run_id` | String | Run identifier |
| `bundle_id` | String | Content-addressed ID (sha256) |
| `tenant_id` | String | Tenant identifier |
| `ingested_at` | String | ISO8601 timestamp |
| `retention_expires_at` | String | ISO8601 timestamp |
| `storage_bytes` | Number | Bundle size |
| `event_count` | Number | Number of events |
| `s3_key` | String | Full S3 object key |
| `verified` | Boolean | Bundle passed verification |
| `manifest` | Map | Cached manifest.json |

**GSI: `run-id-index`**
- PK: `tenant_id`
- SK: `run_id`

**GSI: `ingested-at-index`**
- PK: `tenant_id`
- SK: `ingested_at`

### Authentication & Authorization

#### API Key Structure
```
assay_live_abc123def456...  # Production
assay_test_xyz789...        # Test/sandbox
```

API keys are:
- Scoped to a single tenant
- Stored as salted SHA-256 hashes
- Rate-limited per key (default: 100 req/min)

#### Signed Upload Tokens (Optional)

For large uploads or delegated access, use signed tokens:

```http
POST /v1/upload-tokens
Authorization: Bearer {api_key}
Content-Type: application/json

{
  "run_id": "run_123",
  "expires_in": 3600,
  "max_size_bytes": 104857600
}
```

Response:
```json
{
  "upload_token": "eyJhbGciOiJFUzI1NiIs...",
  "upload_url": "https://store.assay.dev/v1/bundles?token=...",
  "expires_at": "2026-01-28T13:00:00Z"
}
```

Benefits:
- No API key exposure to CI runners
- Time-limited access
- Size-limited uploads
- Auditable token issuance

### Tenant Isolation & Security

#### KMS Key Separation

Each tenant gets a dedicated KMS key for encryption:

```
┌─────────────────────────────────────────────────────────────────┐
│                    KMS Key Hierarchy                             │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Root Key (AWS Managed)                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│              ┌───────────────┼───────────────┐                  │
│              ▼               ▼               ▼                  │
│  ┌─────────────────┐ ┌─────────────────┐ ┌─────────────────┐   │
│  │ Tenant A Key    │ │ Tenant B Key    │ │ Tenant C Key    │   │
│  │ (CMK)           │ │ (CMK)           │ │ (CMK)           │   │
│  └─────────────────┘ └─────────────────┘ └─────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

**Benefits:**
- Tenant A cannot decrypt Tenant B's bundles (even with S3 access)
- Key rotation per tenant
- Audit trail per key
- Cryptographic deletion (destroy key = destroy data)

#### Access Logging

All operations are logged to CloudTrail with tenant context:

```json
{
  "eventName": "PutObject",
  "userIdentity": {
    "type": "AssumedRole",
    "sessionContext": {
      "sessionIssuer": {
        "userName": "assay-evidence-store"
      }
    }
  },
  "requestParameters": {
    "bucketName": "assay-evidence-store-prod",
    "key": "tenant_abc123/bundles/2026/01/..."
  },
  "additionalEventData": {
    "x-assay-tenant-id": "tenant_abc123",
    "x-assay-api-key-id": "key_xyz789"
  }
}
```

#### Authorization Model (OPA Policy)

```rego
package assay.evidence

default allow = false

# Allow ingest if API key is valid for tenant
allow {
    input.action == "ingest"
    input.api_key.tenant_id == input.bundle.tenant_id
    input.api_key.scope == "write"
}

# Allow read if API key is valid for tenant
allow {
    input.action == "read"
    input.api_key.tenant_id == input.bundle.tenant_id
    input.api_key.scope in ["read", "write"]
}
```

### Verification on Ingest

Every bundle is verified before storage:

```rust
async fn ingest_bundle(body: Bytes, tenant_id: &str) -> Result<IngestResponse> {
    // 1. Verify bundle integrity (reuse assay-evidence crate)
    let result = verify_bundle(Cursor::new(&body), VerifyLimits::default())?;

    // 2. Extract metadata from manifest
    let manifest = result.manifest;
    let bundle_id = manifest.bundle_id.clone();

    // 3. Check idempotency (bundle_id already exists?)
    if let Some(existing) = db.get_bundle(&tenant_id, &bundle_id).await? {
        return Ok(IngestResponse::AlreadyExists(existing));
    }

    // 4. Upload to S3 with Object Lock
    let s3_key = format!("{}/bundles/{}/{}/{}.tar.gz",
        tenant_id,
        Utc::now().format("%Y/%m"),
        manifest.run_id,
        bundle_id
    );

    s3.put_object()
        .bucket(&config.bucket)
        .key(&s3_key)
        .body(body.into())
        .object_lock_mode(ObjectLockMode::Compliance)
        .object_lock_retain_until_date(retention_date)
        .send()
        .await?;

    // 5. Store metadata in DynamoDB
    db.put_bundle(BundleRecord { ... }).await?;

    Ok(IngestResponse::Created { ... })
}
```

### Rate Limiting

Default rate limits per API key:
- Ingest: 100 requests/min
- Query: 1000 requests/min
- Burst: 200 requests

Implemented via API Gateway usage plans.

### CloudEvents Observability Integration

Ingest events are emitted for observability:

```json
{
  "specversion": "1.0",
  "type": "assay.evidence.ingested",
  "source": "urn:assay:evidence-store",
  "id": "evt_abc123",
  "time": "2026-01-28T12:00:00Z",
  "data": {
    "tenant_id": "tenant_abc123",
    "bundle_id": "sha256:ade9c15d...",
    "run_id": "run_baseline_001",
    "event_count": 5,
    "storage_bytes": 1078
  }
}
```

These can be routed to:
- Internal analytics (usage metering)
- Customer webhooks (integration triggers)
- SIEM pipelines (security monitoring)

## Alternatives Considered

### 1. GraphQL API

**Pros:**
- Flexible queries
- Strong typing

**Cons:**
- Overkill for simple CRUD
- Larger attack surface
- Caching complexity

**Decision:** REST is simpler and sufficient for MVP.

### 2. gRPC

**Pros:**
- Better performance
- Strong contracts

**Cons:**
- Browser compatibility issues
- Tooling complexity

**Decision:** REST for public API; consider gRPC for internal services later.

### 3. Bucket-per-Tenant

**Pros:**
- Stronger isolation
- Simpler IAM policies

**Cons:**
- Doesn't scale beyond ~100-1000 tenants
- Management overhead

**Decision:** Object key partitioning per AWS best practices.

## Rollout Phases

### Alpha (Week 1-4)
- Single AWS region (us-east-1)
- Single retention policy (90 days)
- Basic API key authentication
- No legal hold (coming in Beta)
- Limited to 10 tenants

### Beta (Week 5-8)
- Per-tenant retention policies
- Legal hold workflows
- Signed upload tokens
- KMS key separation
- Up to 100 tenants

### GA (Q3)
- Multi-region deployment
- Cross-region replication
- Full SLA (99.9%)
- Unlimited tenants
- SOC 2 Type II certification

## Implementation Plan

### Phase 1: MVP (Week 1-2)
- [ ] POST `/v1/bundles` endpoint
- [ ] GET `/v1/bundles/{id}` endpoint
- [ ] GET `/v1/bundles/{id}/download` endpoint
- [ ] API key authentication
- [ ] Basic rate limiting
- [ ] `assay evidence push` CLI command

### Phase 2: Query & Legal Hold (Week 3-4)
- [ ] GET `/v1/bundles` with pagination
- [ ] GET `/v1/runs/{run_id}/bundles` endpoint
- [ ] POST `/v1/bundles/search` endpoint
- [ ] POST `/v1/bundles/{id}/legal-hold` endpoint
- [ ] DynamoDB GSIs for efficient queries
- [ ] `assay evidence pull` and `assay evidence list` CLI commands

### Phase 3: Security Hardening (Week 5-6)
- [ ] Signed upload tokens
- [ ] Per-tenant KMS keys
- [ ] CloudWatch dashboards
- [ ] Alerting on errors/latency
- [ ] Load testing (target: 1000 req/s)

### Phase 4: Production (Week 7-8)
- [ ] Multi-region failover
- [ ] Disaster recovery testing
- [ ] Documentation & SDK examples

## Acceptance Criteria

- [ ] Bundle upload < 500ms p99 latency for 1MB bundles
- [ ] Verification runs on every ingest (no unverified bundles stored)
- [ ] Idempotent uploads (same bundle_id returns 409 with existing record)
- [ ] Rate limiting enforced per API key
- [ ] All operations logged to CloudWatch

## Consequences

### Positive
- Simple, RESTful interface familiar to developers
- Reuses existing `assay-evidence` verification logic
- Scales horizontally via Lambda/containers
- CloudEvents-native for observability integration

### Negative
- DynamoDB query patterns require careful GSI design
- S3 eventual consistency for list operations
- API Gateway costs at high volume

### Neutral
- Must handle S3 multipart upload for large bundles (>5GB)
- Cursor-based pagination required for large result sets

## References

- [AWS Multi-Tenant SaaS API Authorization](https://docs.aws.amazon.com/prescriptive-guidance/latest/saas-multitenant-api-access-authorization/introduction.html)
- [AWS S3 Multi-Tenant Partitioning](https://aws.amazon.com/blogs/apn/partitioning-and-isolating-multi-tenant-saas-data-with-amazon-s3/)
- [CloudEvents Specification](https://cloudevents.io/)
- [Google API Design Guide](https://cloud.google.com/apis/design)
- [ADR-006: Evidence Contract](./ADR-006-Evidence-Contract.md)
- [ADR-009: WORM Storage](./ADR-009-WORM-Storage.md)
