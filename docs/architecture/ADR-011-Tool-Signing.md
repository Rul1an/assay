# ADR-011: MCP Tool Signing with Sigstore

## Status

Proposed (January 2026)

## Context

MCP (Model Context Protocol) tools are vulnerable to supply chain attacks:
- **43% of MCP servers** are vulnerable to command injection (2025 security research)
- Tool definitions can be modified to inject malicious instructions
- No built-in verification mechanism in MCP specification

Assay already has `ToolIdentity` (Phase 9) for hash-based pinning:

```rust
// crates/assay-core/src/mcp/identity.rs
pub struct ToolIdentity {
    pub server_id: String,
    pub tool_name: String,
    pub schema_hash: String,   // SHA-256 of input schema
    pub meta_hash: String,     // SHA-256 of description
}
```

We need to extend this with cryptographic signatures for:
1. **Provenance**: Who published this tool?
2. **Integrity**: Has it been tampered with?
3. **Non-repudiation**: Can we prove authorship?

## Decision

We will implement **Sigstore-based keyless signing** with an `x-assay-sig` extension field in MCP tool definitions.

### Signature Format

```json
{
  "name": "read_file",
  "description": "Read contents of a file",
  "inputSchema": { ... },
  "x-assay-sig": {
    "version": 1,
    "algorithm": "ecdsa-p256",
    "signature": "MEUCIQDx...base64...",
    "certificate": "-----BEGIN CERTIFICATE-----\n...",
    "rekor_entry": "24296fb24b8ad77a...",
    "signed_at": "2026-01-28T12:00:00Z",
    "identity": {
      "issuer": "https://accounts.google.com",
      "subject": "developer@example.com"
    }
  }
}
```

### Architecture

```
┌─────────────────────────────────────────────────────────────────┐
│                     Tool Publisher Workflow                      │
│                                                                  │
│  1. Developer authenticates via OIDC (GitHub, Google, etc.)     │
│  2. Fulcio issues short-lived certificate binding identity       │
│  3. Tool schema is signed with ephemeral key                    │
│  4. Signature + certificate recorded in Rekor transparency log  │
│  5. Tool definition published with x-assay-sig extension        │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                     Tool Consumer Workflow                       │
│                                                                  │
│  1. Assay loads MCP tool definition                             │
│  2. Extract x-assay-sig if present                              │
│  3. Verify signature against certificate                        │
│  4. Verify certificate chain (Fulcio root)                      │
│  5. (Optional) Check Rekor inclusion proof                      │
│  6. Compare identity against policy trust anchors               │
└─────────────────────────────────────────────────────────────────┘
```

### Signing Flow (CLI)

```bash
# Keyless signing (recommended)
assay tool sign --keyless tool-definition.json

# Key-based signing (enterprise)
assay tool sign --key private.pem tool-definition.json

# Verify a signed tool
assay tool verify tool-definition.json

# Verify with explicit trust requirements
assay tool verify tool-definition.json --require-producer-trust policy.yaml

# Verify and require Rekor transparency proof
assay tool verify tool-definition.json --rekor-required
```

### Evidence Verification with Producer Trust

Producer trust can also be verified on evidence bundles:

```bash
# Verify evidence bundle was produced by trusted identity
assay evidence verify bundle.tar.gz --require-producer-trust

# Explicit trust policy
assay evidence verify bundle.tar.gz --trust-policy org-policy.yaml
```

Trust policy file format:
```yaml
# org-policy.yaml
trust_anchors:
  - issuer: "https://token.actions.githubusercontent.com"
    subject: "repo:myorg/*:ref:refs/heads/main"
  - issuer: "https://accounts.google.com"
    subject: "*@mycompany.com"

require_transparency: true
allow_unsigned: false
```

### Verification Logic

```rust
pub struct SignatureVerifier {
    /// Trusted OIDC issuers
    trusted_issuers: Vec<String>,
    /// Trusted email/subject patterns
    trusted_identities: Vec<String>,
    /// Fulcio root certificate (from TUF)
    fulcio_root: Certificate,
    /// Rekor public key (from TUF)
    rekor_key: PublicKey,
}

impl SignatureVerifier {
    pub fn verify(&self, tool: &ToolDefinition) -> Result<VerifyResult, VerifyError> {
        let sig = tool.x_assay_sig.as_ref()
            .ok_or(VerifyError::NoSignature)?;

        // 1. Verify signature over tool content (JCS canonical form)
        let content = canonicalize_tool_jcs(tool)?;
        verify_ecdsa(&sig.signature, &content, &sig.certificate)
            .map_err(|e| VerifyError::SignatureInvalid { reason: e.to_string() })?;

        // 2. Verify certificate chain
        verify_certificate_chain(&sig.certificate, &self.fulcio_root)
            .map_err(|e| VerifyError::CertificateInvalid { reason: e.to_string() })?;

        // 3. Check certificate is not expired
        // (Fulcio certs are short-lived, but Rekor proves signing time)

        // 4. Verify identity against trust policy
        if !self.is_trusted_identity(&sig.identity) {
            return Err(VerifyError::ProducerUntrusted {
                identity: sig.identity.subject.clone(),
                issuer: sig.identity.issuer.clone(),
                reason: "Identity not in trust_anchors".into(),
            });
        }

        // 5. (Optional) Verify Rekor inclusion
        if let Some(entry_id) = &sig.rekor_entry {
            verify_rekor_inclusion(entry_id, &sig.signature)
                .map_err(|e| VerifyError::RekorInclusionFailed { reason: e.to_string() })?;
        }

        Ok(VerifyResult::Verified {
            identity: sig.identity.clone(),
            signed_at: sig.signed_at,
        })
    }
}

/// Verification error codes for stable API contracts
#[derive(Debug, Clone)]
pub enum VerifyError {
    /// Tool has no x-assay-sig field
    NoSignature,

    /// Signature does not match content
    SignatureInvalid { reason: String },

    /// Certificate chain validation failed
    CertificateInvalid { reason: String },

    /// Certificate has expired (and no Rekor timestamp)
    CertificateExpired { expired_at: DateTime<Utc> },

    /// Identity not in trust policy
    ProducerUntrusted {
        identity: String,
        issuer: String,
        reason: String,
    },

    /// Rekor inclusion proof failed
    RekorInclusionFailed { reason: String },

    /// Rekor entry not found
    RekorEntryNotFound { entry_id: String },
}

impl VerifyError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::NoSignature => "E_NO_SIGNATURE",
            Self::SignatureInvalid { .. } => "E_SIGNATURE_INVALID",
            Self::CertificateInvalid { .. } => "E_CERTIFICATE_INVALID",
            Self::CertificateExpired { .. } => "E_CERTIFICATE_EXPIRED",
            Self::ProducerUntrusted { .. } => "E_PRODUCER_UNTRUSTED",
            Self::RekorInclusionFailed { .. } => "E_REKOR_INCLUSION_FAILED",
            Self::RekorEntryNotFound { .. } => "E_REKOR_ENTRY_NOT_FOUND",
        }
    }
}
```

### Policy Integration

```yaml
# assay.yaml
tool_verification:
  mode: strict  # strict | warn | disabled

  trust_anchors:
    # Trust specific identities
    - issuer: "https://github.com/login/oauth"
      subject: "repo:myorg/mcp-tools:ref:refs/heads/main"

    # Trust all from an issuer
    - issuer: "https://accounts.google.com"
      subject: "*@mycompany.com"

  # Require Rekor transparency log proof
  require_transparency: true

  # Allow unsigned tools (for development)
  allow_unsigned:
    - "localhost:*"
    - "test-*"
```

### Content to Sign

The signature covers a **canonical representation** of:

```json
{
  "name": "read_file",
  "description": "Read contents of a file",
  "inputSchema": {
    "type": "object",
    "properties": {
      "path": { "type": "string" }
    },
    "required": ["path"]
  }
}
```

**Canonicalization:** JCS (RFC 8785) - same as Evidence Contract.

**What's NOT signed:**
- `x-assay-sig` itself (obviously)
- Runtime metadata added by MCP servers
- Any fields not in the canonical set

### Sigstore Integration

#### Fulcio (Certificate Authority)

```
POST https://fulcio.sigstore.dev/api/v2/signingCert
Authorization: Bearer {oidc_token}

{
  "publicKeyRequest": {
    "publicKey": {
      "algorithm": "ECDSA",
      "content": "MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAE..."
    },
    "proofOfPossession": "MEUCIQD..."
  }
}
```

#### Rekor (Transparency Log)

```
POST https://rekor.sigstore.dev/api/v1/log/entries

{
  "kind": "hashedrekord",
  "apiVersion": "0.0.1",
  "spec": {
    "signature": {
      "content": "MEUCIQDx...",
      "publicKey": { "content": "MFkw..." }
    },
    "data": {
      "hash": { "algorithm": "sha256", "value": "abc123..." }
    }
  }
}
```

### SchemaPin Compatibility

We align with [SchemaPin](https://github.com/ThirdKeyAI/SchemaPin) protocol where possible:
- Same canonical JSON format
- Compatible signature algorithm (ECDSA P-256)
- Similar trust anchor model

This enables interoperability with other MCP security tools.

## Alternatives Considered

### 1. Simple Hash Pinning (Current State)

**Pros:**
- Already implemented
- No external dependencies

**Cons:**
- No provenance (who published?)
- Manual hash distribution
- No revocation mechanism

**Decision:** Keep as fallback, extend with signatures.

### 2. GPG Signatures

**Pros:**
- Well-understood
- Existing tooling

**Cons:**
- Key management burden
- No built-in transparency
- Complex trust model

**Decision:** Too much operational overhead.

### 3. Custom PKI

**Pros:**
- Full control
- No external dependencies

**Cons:**
- Must operate CA
- No ecosystem adoption
- Trust bootstrap problem

**Decision:** Sigstore provides this as a service.

### 4. JWT-based Attestation

**Pros:**
- Familiar format
- OIDC integration

**Cons:**
- Not designed for artifact signing
- No transparency log

**Decision:** Sigstore uses OIDC but produces proper X.509 certificates.

## Implementation Plan

### Phase 1: Signing CLI (Week 1)
- [ ] `assay tool sign --keyless` command
- [ ] Fulcio integration for certificate issuance
- [ ] Rekor integration for transparency logging
- [ ] `x-assay-sig` serialization

### Phase 2: Verification (Week 2)
- [ ] `assay tool verify` command
- [ ] Policy-based trust anchors
- [ ] Integration with existing `ToolIdentity`
- [ ] Warning mode for gradual rollout

### Phase 3: MCP Server Integration (Week 3)
- [ ] Verify tools on MCP server startup
- [ ] Runtime policy enforcement
- [ ] Unsigned tool handling (warn/block)

### Phase 4: Documentation (Week 4)
- [ ] Publisher guide
- [ ] Verifier guide
- [ ] Trust policy examples

## Acceptance Criteria

- [ ] Keyless signing produces valid `x-assay-sig`
- [ ] Verification passes for validly signed tools
- [ ] Verification fails for tampered tools
- [ ] Trust policy correctly filters untrusted identities
- [ ] Rekor inclusion proof is verifiable
- [ ] CLI UX matches `cosign sign-blob` / `cosign verify-blob`

## Consequences

### Positive
- Provenance for all tool definitions
- No key management for developers (keyless)
- Transparency via public Rekor log
- Interoperable with Sigstore ecosystem

### Negative
- External dependency (Sigstore infrastructure)
- OIDC login required for signing
- Verification adds latency (~100ms per tool)

### Neutral
- Must distribute Fulcio/Rekor roots via TUF
- Certificate expiry handled by Rekor timestamp

## References

- [Sigstore Cosign Overview](https://docs.sigstore.dev/cosign/signing/overview/)
- [Fulcio OIDC Usage](https://docs.sigstore.dev/certificate_authority/oidc-in-fulcio)
- [Rekor Transparency Log](https://docs.sigstore.dev/logging/overview/)
- [SchemaPin Protocol](https://github.com/ThirdKeyAI/SchemaPin)
- [MCP Security Best Practices](https://modelcontextprotocol.io/specification/2025-11-25/basic/security_best_practices)
- [SLSA Provenance](https://slsa.dev/provenance)
- [JCS RFC 8785](https://www.rfc-editor.org/rfc/rfc8785)
