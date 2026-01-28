# ADR-009: WORM Storage for Evidence Retention

## Status

Proposed (January 2026)

## Context

Assay Evidence Bundles require immutable, tamper-evident storage for compliance with:
- **EU AI Act Article 12**: "High-risk AI systems shall technically allow for automatic recording of events (logs) over the lifetime of the system"
- **SEC Rule 17a-4**: Broker-dealer recordkeeping requirements
- **CFTC/FINRA**: Financial services compliance

The Evidence Store MVP needs WORM (Write Once Read Many) storage to provide:
1. Immutability guarantees for audit trails
2. Regulatory compliance certification
3. Legal hold capabilities for investigations

## Decision

We will use **Amazon S3 Object Lock in Compliance Mode** as the primary WORM storage backend.

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                    Evidence Store Ingest API                     │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│                      S3 Bucket Configuration                     │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Object Lock: ENABLED                                     │   │
│  │ Mode: COMPLIANCE (cannot be overridden by any user)      │   │
│  │ Default Retention: 90 days                               │   │
│  │ Versioning: ENABLED (required for Object Lock)           │   │
│  └─────────────────────────────────────────────────────────┘   │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ Object Key Schema:                                       │   │
│  │ /{tenant_id}/bundles/{run_id}/{bundle_id}.tar.gz        │   │
│  │                                                          │   │
│  │ Metadata:                                                │   │
│  │ - x-amz-meta-run-id: {run_id}                           │   │
│  │ - x-amz-meta-bundle-id: {bundle_id} (sha256)            │   │
│  │ - x-amz-meta-tenant-id: {tenant_id}                     │   │
│  │ - x-amz-meta-ingested-at: {ISO8601 timestamp}           │   │
│  └─────────────────────────────────────────────────────────┘   │
└─────────────────────────────────────────────────────────────────┘
```

### Retention Tiers

| Tier | Retention | Use Case | Compliance |
|------|-----------|----------|------------|
| **Standard** | 90 days | Default for all bundles | EU AI Act Article 12 |
| **Extended** | 1 year | Financial services | SEC 17a-4 baseline |
| **Regulatory** | 7 years | Broker-dealer records | SEC 17a-4(f) |
| **Legal Hold** | Indefinite | Active investigations | All |

### S3 Configuration

```yaml
# CloudFormation / Terraform equivalent
EvidenceBucket:
  Type: AWS::S3::Bucket
  Properties:
    BucketName: assay-evidence-store-${Environment}
    VersioningConfiguration:
      Status: Enabled
    ObjectLockEnabled: true
    ObjectLockConfiguration:
      ObjectLockEnabled: Enabled
      Rule:
        DefaultRetention:
          Mode: COMPLIANCE
          Days: 90
    PublicAccessBlockConfiguration:
      BlockPublicAcls: true
      BlockPublicPolicy: true
      IgnorePublicAcls: true
      RestrictPublicBuckets: true
    BucketEncryption:
      ServerSideEncryptionConfiguration:
        - ServerSideEncryptionByDefault:
            SSEAlgorithm: aws:kms
            KMSMasterKeyID: !Ref EvidenceKMSKey
```

### Legal Hold API

```
PUT /v1/bundles/{bundle_id}/legal-hold
Authorization: Bearer {token}
Content-Type: application/json

{
  "enabled": true,
  "reason": "Investigation case #12345",
  "requested_by": "legal@example.com"
}
```

### Delete Semantics

| Mode | DELETE Behavior | Use Case |
|------|-----------------|----------|
| **Governance** | Soft delete (versioned, recoverable by root user) | Development/staging |
| **Compliance** | DELETE disabled until retention expires | Production, regulated |
| **Legal Hold** | DELETE blocked indefinitely (overrides retention) | Active investigations |

**API Behavior:**

```http
DELETE /v1/bundles/{bundle_id}
```

| Condition | Response | Effect |
|-----------|----------|--------|
| Governance mode, no legal hold | `200 OK` | Soft delete (version marker) |
| Compliance mode, retention active | `403 Forbidden` | No effect |
| Any mode with legal hold | `403 Forbidden` | No effect |
| Compliance mode, retention expired | `200 OK` | Permanent delete |

**Important:** In Compliance mode, even AWS root users cannot delete objects before retention expires. This is a regulatory requirement.

## Alternatives Considered

### 1. AWS QLDB (Quantum Ledger Database)

**Pros:**
- Native Merkle tree verification
- Built-in cryptographic digest
- SQL-like query interface

**Cons:**
- Higher cost ($0.65/million requests vs $0.005/1000 requests for S3)
- Limited ecosystem integration
- AWS-only (no multi-cloud)

**Decision:** Not selected. S3 Object Lock provides sufficient guarantees at lower cost.

### 2. Azure Immutable Blob Storage

**Pros:**
- Similar compliance certifications
- Multi-region replication

**Cons:**
- Vendor lock-in if we start with AWS
- Different API semantics

**Decision:** Consider as future multi-cloud option.

### 3. Custom Merkle Chain on PostgreSQL

**Pros:**
- Full control over verification logic
- No cloud dependency

**Cons:**
- Must build and certify compliance ourselves
- Operational burden
- No independent audit certification

**Decision:** Not selected. Regulatory certification is a hard requirement.

## Compliance Certifications

S3 Object Lock has been independently assessed by **Cohasset Associates** for:

| Regulation | Requirement | S3 Object Lock Status |
|------------|-------------|----------------------|
| SEC Rule 17a-4(f) | Non-erasable, non-rewritable media | ✅ Compliant |
| SEC Rule 18a-6 | Security-based swap dealer records | ✅ Compliant |
| CFTC Rule 1.31 | Commodity trading records | ✅ Compliant |
| FINRA Rule 4511 | Books and records | ✅ Compliant |

AWS provides contractual addenda for these requirements.

## EU AI Act Article 12 Mapping

| Article 12 Requirement | Implementation |
|------------------------|----------------|
| "Automatic recording of events" | Evidence bundles with CloudEvents format |
| "Over the lifetime of the system" | 90-day default + configurable retention |
| "Identify situations presenting risk" | Lint findings, diff results in bundles |
| "Post-market monitoring" | Query API for trend analysis |
| "Recording of each use period" | `assay.profile.started` / `finished` events |

**Note:** Draft standard prEN ISO/IEC 24970 (AI System Logging) is expected in 2026 and may require adjustments.

## Implementation Plan

### Phase 1: MVP (Week 1-2)
- [ ] Create S3 bucket with Object Lock enabled
- [ ] Implement basic PUT endpoint for bundle upload
- [ ] Add retention period header handling
- [ ] Deploy to staging environment

### Phase 2: Compliance (Week 3-4)
- [ ] Add legal hold API endpoints
- [ ] Implement retention tier selection
- [ ] Create compliance audit logs (CloudTrail)
- [ ] Document SEC 17a-4 procedures

### Phase 3: Multi-Region (Q3)
- [ ] Cross-region replication for disaster recovery
- [ ] Multi-cloud support (Azure Immutable Blob)

## Acceptance Criteria

- [ ] Bundles cannot be deleted before retention period expires
- [ ] Legal hold prevents deletion indefinitely
- [ ] All operations logged to CloudTrail
- [ ] Encryption at rest (KMS) and in transit (TLS 1.3)
- [ ] Cohasset compliance letter available for customers

## Consequences

### Positive
- Regulatory compliance out-of-the-box
- No custom verification logic needed
- Independent certification (Cohasset)
- Cost-effective ($0.023/GB/month for S3 Standard)

### Negative
- AWS lock-in for initial implementation
- Cannot truly delete data during retention (even if requested)
- Storage costs accumulate over retention period

### Neutral
- Versioning required (minor storage overhead)
- Must handle "object already exists" for idempotency

## References

- [AWS S3 Object Lock Documentation](https://docs.aws.amazon.com/AmazonS3/latest/userguide/object-lock.html)
- [SEC Rule 17a-4 AWS Compliance](https://aws.amazon.com/compliance/secrule17a-4f/)
- [EU AI Act Article 12](https://artificialintelligenceact.eu/article/12/)
- [Cohasset Associates Assessment](https://d1.awsstatic.com/whitepapers/compliance/AWS_SEC_Rule_17a-4_Compliance_Assessment.pdf)
- [Draft prEN ISO/IEC 24970](https://www.iso.org/standard/79799.html) (AI System Logging)
