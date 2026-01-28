# Tool Signing Specification v1

**Status:** Draft (January 2026)
**Scope:** Local ed25519 signing for MCP tool definitions

---

## 1. Overview

This specification defines the `x-assay-sig` extension field for cryptographically signing MCP tool definitions. It enables:

1. **Integrity** - Detect tampering of tool definitions
2. **Provenance** - Verify who signed the tool
3. **Trust policies** - Enforce organizational signing requirements

### Design Principles

- **Deterministic** - Same tool definition always produces same signing input
- **Self-contained** - Signature can be verified offline
- **DSSE-aligned** - Compatible with future Sigstore/in-toto migration
- **Minimal** - No external dependencies for basic verification

---

## 2. Signing Domain

### 2.1 Signing Input

The signing input is the **JCS-canonicalized** tool definition with the `x-assay-sig` field removed.

```
Signing Input = JCS(tool_object - {"x-assay-sig"})
```

**JCS (JSON Canonicalization Scheme, RFC 8785):**
- Keys sorted lexicographically
- No whitespace
- Numbers in shortest form
- Unicode normalized

### 2.2 What Is Signed

| Field | Included in Signing Input |
|-------|---------------------------|
| `name` | Yes |
| `description` | Yes |
| `inputSchema` | Yes |
| `x-assay-sig` | **No** (removed before canonicalization) |

### 2.3 Payload Type Binding

To prevent type confusion attacks, the signature binds to a payload type:

```
PAE = "DSSEv1" || len(payload_type) || payload_type || len(payload) || payload
```

Where:
- `payload_type` = `"application/vnd.assay.tool+json;v=1"`
- `payload` = JCS-canonicalized tool definition (without `x-assay-sig`)

**Note:** The PAE (Pre-Authentication Encoding) format follows DSSE specification for future compatibility.

---

## 3. Signature Format

### 3.1 x-assay-sig Object

```json
{
  "version": 1,
  "algorithm": "ed25519",
  "payload_type": "application/vnd.assay.tool+json;v=1",
  "payload_digest": "sha256:abc123def456...",
  "key_id": "sha256:789xyz...",
  "signature": "base64-encoded-ed25519-signature",
  "signed_at": "2026-01-28T12:00:00Z",
  "public_key": "base64-encoded-spki-pubkey"
}
```

### 3.2 Field Definitions

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `version` | integer | Yes | Schema version. Must be `1`. |
| `algorithm` | string | Yes | Signature algorithm. Must be `"ed25519"` for v1. |
| `payload_type` | string | Yes | Content type of signed payload. Must be `"application/vnd.assay.tool+json;v=1"`. |
| `payload_digest` | string | Yes | SHA-256 of canonical payload: `sha256:<hex>`. |
| `key_id` | string | Yes | SHA-256 of SPKI-encoded public key: `sha256:<hex>`. |
| `signature` | string | Yes | Base64-encoded ed25519 signature over PAE. |
| `signed_at` | string | Yes | ISO 8601 timestamp of signing. Not part of signed content. |
| `public_key` | string | No | Base64-encoded SPKI public key. Optional; for convenience only. |

### 3.3 Key ID Computation

```
key_id = "sha256:" || hex(SHA256(spki_bytes))
```

Where `spki_bytes` is the DER-encoded SubjectPublicKeyInfo.

---

## 4. Key Format

### 4.1 Private Key

- **Format:** PKCS#8 PEM
- **Header:** `-----BEGIN PRIVATE KEY-----`
- **File permissions:** `0600` (owner read/write only)
- **File extension:** `.pem`

### 4.2 Public Key

- **Format:** SPKI PEM (SubjectPublicKeyInfo)
- **Header:** `-----BEGIN PUBLIC KEY-----`
- **File extension:** `.pem`

### 4.3 Example Key Generation

```bash
# Using assay CLI
assay tool keygen --out ~/.assay/keys/

# Output:
#   ~/.assay/keys/private_key.pem (PKCS#8, mode 0600)
#   ~/.assay/keys/public_key.pem (SPKI)
#   key_id: sha256:abc123def456...
```

---

## 5. Signing Process

### 5.1 Algorithm

```
1. Parse tool definition as JSON object T
2. Remove T["x-assay-sig"] if present
3. Compute canonical = JCS(T)
4. Compute payload_type = "application/vnd.assay.tool+json;v=1"
5. Compute PAE = DSSEv1_PAE(payload_type, canonical)
6. Sign: signature = ed25519_sign(private_key, PAE)
7. Compute payload_digest = "sha256:" + hex(SHA256(canonical))
8. Compute key_id = "sha256:" + hex(SHA256(public_key_spki))
9. Build x-assay-sig object
10. Set T["x-assay-sig"] = x-assay-sig
11. Output T
```

### 5.2 PAE Encoding (DSSE-compatible)

```
PAE(type, payload) =
    "DSSEv1 " ||
    len(type) as 8-byte little-endian ||
    type ||
    len(payload) as 8-byte little-endian ||
    payload
```

---

## 6. Verification Process

### 6.1 Algorithm

```
1. Parse tool definition as JSON object T
2. Extract sig = T["x-assay-sig"]
3. If sig is missing:
   - If policy requires signature: FAIL (exit 2)
   - Else: PASS (unsigned allowed)
4. Validate sig.version == 1
5. Validate sig.algorithm == "ed25519"
6. Validate sig.payload_type == "application/vnd.assay.tool+json;v=1"
7. Remove T["x-assay-sig"]
8. Compute canonical = JCS(T)
9. Verify: payload_digest == "sha256:" + hex(SHA256(canonical))
10. Compute PAE = DSSEv1_PAE(sig.payload_type, canonical)
11. Obtain public key:
    - From trust policy by key_id, OR
    - From sig.public_key if --allow-embedded-key
12. Verify: ed25519_verify(public_key, PAE, base64_decode(sig.signature))
13. If signature invalid: FAIL (exit 4)
14. Compute actual_key_id from public key
15. If actual_key_id != sig.key_id: FAIL (exit 4)
16. Check trust policy:
    - If key_id in trusted_key_ids: PASS
    - If key_id matches trusted_keys[].key_id: PASS
    - Else: FAIL (exit 3)
17. PASS (exit 0)
```

### 6.2 Exit Codes

| Code | Meaning | When |
|------|---------|------|
| 0 | Success | Signature valid and key trusted |
| 1 | Error | I/O error, malformed JSON, invalid format |
| 2 | Unsigned | No signature when policy requires one |
| 3 | Untrusted | Valid signature but key not in trust policy |
| 4 | Invalid | Bad signature, wrong payload_type, digest mismatch |

---

## 7. Trust Policy

### 7.1 Format (YAML)

```yaml
# Require all tools to be signed
require_signed: true

# Simple list of trusted key IDs
trusted_key_ids:
  - "sha256:abc123..."
  - "sha256:def456..."

# Detailed trusted keys with metadata
trusted_keys:
  - key_id: "sha256:789xyz..."
    name: "CI Signing Key"
    public_key_path: "./keys/ci-public.pem"
```

### 7.2 Policy Evaluation

1. If `require_signed: true` and tool is unsigned → reject
2. Extract `key_id` from signature
3. Check if `key_id` in `trusted_key_ids` → accept
4. Check if `key_id` matches any `trusted_keys[].key_id` → accept
5. Otherwise → reject as untrusted

---

## 8. Security Considerations

### 8.1 Key Management

- Private keys MUST be stored with mode `0600`
- Private keys SHOULD NOT be committed to version control
- Use CI secrets or key management systems for automated signing

### 8.2 Type Confusion Prevention

The `payload_type` field prevents attacks where a valid signature for one type of document is reused for another. Verification MUST fail if `payload_type` doesn't match the expected value.

### 8.3 Key ID vs Embedded Public Key

- `key_id` is the authoritative identifier for trust decisions
- `public_key` field is optional and for convenience only
- Trust policies SHOULD use `key_id` matching, not embedded keys
- `--allow-embedded-key` is for development/testing only

### 8.4 Replay Protection

This specification does not include replay protection. The `signed_at` timestamp is metadata only and not cryptographically bound. For replay-sensitive use cases, include a nonce or use transparency logs (future Sigstore integration).

---

## 9. Examples

### 9.1 Unsigned Tool

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

### 9.2 Signed Tool

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
  },
  "x-assay-sig": {
    "version": 1,
    "algorithm": "ed25519",
    "payload_type": "application/vnd.assay.tool+json;v=1",
    "payload_digest": "sha256:e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
    "key_id": "sha256:a1b2c3d4e5f6...",
    "signature": "MEUCIQDx...",
    "signed_at": "2026-01-28T12:00:00Z"
  }
}
```

### 9.3 Canonical Form (Signing Input)

For the tool above, the JCS canonical form (signing input) is:

```json
{"description":"Read contents of a file","inputSchema":{"properties":{"path":{"type":"string"}},"required":["path"],"type":"object"},"name":"read_file"}
```

---

## 10. Future Extensions

### 10.1 Sigstore Integration (Enterprise)

v2 will add:
- `algorithm: "ecdsa-p256"` for Sigstore
- `certificate` field for Fulcio short-lived certs
- `rekor_entry` field for transparency log proof
- `identity` object with OIDC issuer/subject

### 10.2 Tool Bundles

Future versions may support signing multiple tools in a bundle with a single signature.

---

## 11. References

- [RFC 8785: JSON Canonicalization Scheme (JCS)](https://www.rfc-editor.org/rfc/rfc8785)
- [DSSE: Dead Simple Signing Envelope](https://github.com/secure-systems-lab/dsse)
- [ed25519](https://ed25519.cr.yp.to/)
- [PKCS#8](https://datatracker.ietf.org/doc/html/rfc5958)
- [SPKI](https://datatracker.ietf.org/doc/html/rfc5280)
- [Sigstore](https://docs.sigstore.dev/)
