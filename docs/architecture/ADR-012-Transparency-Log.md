# ADR-012: Transparency Log Integration with Rekor

## Status

Proposed (January 2026)

## Context

[ADR-011](./ADR-011-Tool-Signing.md) introduces Sigstore-based tool signing. A critical component is the **transparency log** (Rekor) which provides:

1. **Immutable record** of all signing events
2. **Inclusion proofs** to verify signatures were recorded
3. **Consistency proofs** to detect log tampering
4. **Identity monitoring** to detect account compromises

This ADR details the Rekor integration for verification and monitoring.

## Decision

We will integrate with **public Rekor** (rekor.sigstore.dev) for open source, with optional private instance support for enterprise.

### Verification Flow

```
┌─────────────────────────────────────────────────────────────────┐
│                    Signature Verification                        │
│                                                                  │
│  Input: tool definition with x-assay-sig                        │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 1. Extract rekor_entry UUID from x-assay-sig            │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 2. Fetch entry from Rekor API                           │   │
│  │    GET /api/v1/log/entries/{uuid}                       │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 3. Verify inclusion proof (Merkle tree)                 │   │
│  │    - Entry hash matches tree leaf                       │   │
│  │    - Proof path to signed tree head                     │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 4. Verify signed tree head (STH)                        │   │
│  │    - STH signature from Rekor public key                │   │
│  │    - Timestamp within acceptable window                 │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│  ┌─────────────────────────────────────────────────────────┐   │
│  │ 5. Match entry content to tool signature                │   │
│  │    - Same artifact hash                                 │   │
│  │    - Same signature bytes                               │   │
│  │    - Same certificate                                   │   │
│  └─────────────────────────────────────────────────────────┘   │
│                              │                                   │
│                              ▼                                   │
│                    ✅ Verification Complete                      │
└─────────────────────────────────────────────────────────────────┘
```

### Rekor API Integration

#### Fetch Entry

```rust
pub async fn fetch_rekor_entry(uuid: &str) -> Result<RekorEntry> {
    let url = format!("{}/api/v1/log/entries/{}", REKOR_URL, uuid);
    let response: HashMap<String, LogEntry> = client.get(&url).send().await?.json().await?;

    let (entry_uuid, entry) = response.into_iter().next()
        .ok_or(Error::EntryNotFound)?;

    Ok(RekorEntry {
        uuid: entry_uuid,
        body: entry.body,
        integrated_time: entry.integrated_time,
        log_id: entry.log_id,
        log_index: entry.log_index,
        verification: entry.verification,
    })
}
```

#### Verify Inclusion Proof

```rust
pub fn verify_inclusion_proof(
    entry: &RekorEntry,
    rekor_public_key: &PublicKey,
) -> Result<()> {
    let proof = &entry.verification.inclusion_proof;

    // 1. Compute leaf hash
    let leaf_hash = sha256(&entry.body);

    // 2. Walk Merkle proof to root
    let computed_root = proof.hashes.iter()
        .fold(leaf_hash, |acc, sibling| {
            if proof.log_index & 1 == 0 {
                sha256(&[acc, sibling.clone()].concat())
            } else {
                sha256(&[sibling.clone(), acc].concat())
            }
        });

    // 3. Verify signed tree head
    let sth = &proof.signed_tree_head;
    verify_ecdsa(
        &sth.signature,
        &format!("{}{}", sth.tree_size, sth.root_hash),
        rekor_public_key,
    )?;

    // 4. Compare roots
    if computed_root != sth.root_hash {
        return Err(Error::InclusionProofFailed);
    }

    Ok(())
}
```

### Offline Verification Bundle

For air-gapped environments, we support **offline bundles** containing all verification material:

```json
{
  "x-assay-sig": {
    "signature": "...",
    "certificate": "...",
    "rekor_bundle": {
      "entry": { ... },
      "inclusion_proof": {
        "log_index": 12345678,
        "root_hash": "abc123...",
        "tree_size": 50000000,
        "hashes": ["def456...", "ghi789..."],
        "signed_tree_head": {
          "signature": "...",
          "timestamp": "2026-01-28T12:00:00Z"
        }
      }
    }
  }
}
```

This allows verification without network access to Rekor.

### Identity Monitoring

Beyond one-time verification, we support **continuous monitoring** for security teams:

```
┌─────────────────────────────────────────────────────────────────┐
│                    Identity Monitoring Service                   │
│                                                                  │
│  Watches Rekor for signing events matching:                     │
│  - Organization email domains                                   │
│  - GitHub repository patterns                                   │
│  - Specific OIDC subjects                                       │
│                                                                  │
│  Alerts on:                                                     │
│  - Unexpected signing events (account compromise)               │
│  - Signing from unknown locations                               │
│  - Signing outside business hours                               │
│  - Revoked certificates used before revocation                  │
└─────────────────────────────────────────────────────────────────┘
```

#### Using rekor-monitor

```bash
# Monitor for specific identity
rekor-monitor \
  --identity "developer@mycompany.com" \
  --oidc-issuer "https://accounts.google.com" \
  --output-file alerts.json

# Monitor GitHub Actions workflow
rekor-monitor \
  --identity "repo:myorg/mcp-tools:ref:refs/heads/main" \
  --oidc-issuer "https://token.actions.githubusercontent.com"
```

### Consistency Verification

To detect log tampering, we periodically verify **consistency proofs**:

```rust
pub async fn verify_consistency(
    prev_checkpoint: &Checkpoint,
    rekor_public_key: &PublicKey,
) -> Result<Checkpoint> {
    // 1. Fetch current log info
    let log_info = fetch_log_info().await?;

    // 2. Get consistency proof between old and new tree sizes
    let proof = fetch_consistency_proof(
        prev_checkpoint.tree_size,
        log_info.tree_size,
    ).await?;

    // 3. Verify proof
    verify_consistency_proof(
        &prev_checkpoint.root_hash,
        &log_info.root_hash,
        prev_checkpoint.tree_size,
        log_info.tree_size,
        &proof.hashes,
    )?;

    // 4. Verify signed tree head
    verify_ecdsa(
        &log_info.signed_tree_head.signature,
        &format!("{}{}", log_info.tree_size, log_info.root_hash),
        rekor_public_key,
    )?;

    Ok(Checkpoint {
        tree_size: log_info.tree_size,
        root_hash: log_info.root_hash,
        timestamp: Utc::now(),
    })
}
```

### TUF Root Distribution

Rekor's public key and Fulcio's root certificate are distributed via [The Update Framework (TUF)](https://theupdateframework.io/):

```rust
pub fn initialize_sigstore_roots() -> Result<TrustRoot> {
    // TUF repository: tuf-repo.sigstore.dev
    let tuf_client = TufClient::new("https://tuf-repo.sigstore.dev")?;

    // Fetch trusted root
    let root = tuf_client.get_target("trusted_root.json")?;

    Ok(TrustRoot {
        fulcio_root: root.certificate_authorities[0].clone(),
        rekor_keys: root.transparency_logs
            .iter()
            .map(|tl| tl.public_key.clone())
            .collect(),
        valid_from: root.valid_from,
    })
}
```

### Caching Strategy

To minimize latency and API calls:

| Data | Cache TTL | Location |
|------|-----------|----------|
| TUF root | 24 hours | Disk (XDG_CACHE) |
| Rekor entries | Indefinite | Disk (content-addressed) |
| Signed tree heads | 5 minutes | Memory |
| Inclusion proofs | Indefinite | Disk (with entry) |

```rust
pub struct RekorCache {
    entries: DiskCache<String, RekorEntry>,
    sth_cache: RwLock<Option<(Instant, SignedTreeHead)>>,
}

impl RekorCache {
    pub async fn get_entry(&self, uuid: &str) -> Result<RekorEntry> {
        // Check disk cache first
        if let Some(entry) = self.entries.get(uuid)? {
            return Ok(entry);
        }

        // Fetch from API
        let entry = fetch_rekor_entry(uuid).await?;

        // Cache to disk (entries are immutable)
        self.entries.put(uuid, &entry)?;

        Ok(entry)
    }
}
```

## Alternatives Considered

### 1. No Transparency Log (Signature Only)

**Pros:**
- Simpler implementation
- No external dependency

**Cons:**
- No proof of signing time
- No detection of compromised keys
- Cannot revoke signatures

**Decision:** Transparency is essential for supply chain security.

### 2. Private-Only Rekor

**Pros:**
- Full control
- No public disclosure

**Cons:**
- Operational burden
- No cross-organization trust

**Decision:** Public Rekor for open source, private option for enterprise.

### 3. Alternative Transparency Logs

**Options:**
- Google Trillian
- Certificate Transparency (CT) logs
- AWS QLDB

**Decision:** Rekor is purpose-built for Sigstore and has the best ecosystem integration.

### CLI Flags

```bash
# Require Rekor transparency proof for verification
assay tool verify tool.json --rekor-required

# Verify evidence bundle with Rekor check
assay evidence verify bundle.tar.gz --rekor-required

# Skip Rekor check (offline mode)
assay tool verify tool.json --offline
assay evidence verify bundle.tar.gz --offline
```

**Policy-based configuration:**

```yaml
# assay.yaml
tool_verification:
  require_transparency: true  # Equivalent to --rekor-required

evidence_verification:
  require_transparency: true
```

## Implementation Plan

### Phase 1: Basic Verification (Week 1)
- [ ] Rekor API client
- [ ] Entry fetching
- [ ] Inclusion proof verification
- [ ] TUF root initialization

### Phase 2: Offline Bundles (Week 2)
- [ ] Bundle format specification
- [ ] Bundle generation during signing
- [ ] Offline verification path

### Phase 3: Caching (Week 2)
- [ ] Disk cache for entries
- [ ] Memory cache for STH
- [ ] Cache invalidation logic

### Phase 4: Monitoring (Week 3-4)
- [ ] rekor-monitor wrapper
- [ ] Identity pattern matching
- [ ] Alert generation

## Acceptance Criteria

- [ ] Inclusion proof verification passes for valid entries
- [ ] Tampered entries fail verification
- [ ] Offline bundles verify without network
- [ ] Cache hit rate > 90% for repeated verifications
- [ ] Verification latency < 200ms (cached), < 500ms (uncached)

## Consequences

### Positive
- Cryptographic proof of signing time
- Detection of compromised identities
- Append-only audit trail
- No trust in single entity (distributed witnesses)

### Negative
- Dependency on Rekor availability (99.5% SLA)
- Network latency for uncached verifications
- Storage for cached entries

### Neutral
- Must update TUF roots periodically
- Rekor v2 migration planned for 2026

## References

- [Rekor Overview](https://docs.sigstore.dev/logging/overview/)
- [Rekor CLI](https://docs.sigstore.dev/logging/cli/)
- [rekor-monitor](https://blog.sigstore.dev/using-rekor-monitor)
- [The Update Framework (TUF)](https://theupdateframework.io/)
- [Merkle Tree Proofs](https://transparency.dev/verifiable-data-structures/)
- [OpenSSF rekor-monitor Production Readiness](https://openssf.org/blog/2025/12/19/catching-malicious-package-releases-using-a-transparency-log/)
- [ADR-011: Tool Signing](./ADR-011-Tool-Signing.md)
