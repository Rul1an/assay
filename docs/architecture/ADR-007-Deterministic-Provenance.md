# ADR-007: Deterministic Identification & Provenance

## Status
Proposed (Q1 2026 Strategy)

## Context
Evidence is only valuable if it is reproducible and verifiable. We need a way to generate IDs that remain stable across platform restarts and can be independently verified by third parties.

## Decision
Assay will implement a deterministic identification scheme based on canonical JSON and Blake3 hashing.

### 1. Stable Addressing
- **Run ID**: Uses **UUID v7** (time-ordered) to ensure sequential storage optimization in Evidence Stores while remaining globally unique.
- **Event ID**: `base32(blake3(canonical_json(envelope + context + payload)))`.
- **Decision ID**: `base32(blake3(policy_digest + tool_name + normalized_args + context_fingerprint))`.

### 2. Canonicalization (JCS)
All JSON payloads must be processed using **RFC 8785 (JSON Canonicalization Scheme)** before hashing. This ensures that field order or whitespace changes do not break integrity checks.

### 3. Cryptographic Provenance (Anti-Poisoning)
To prevent "Tool Poisoning", every `tool` event must include:
- `tool_digest`: Hash of the tool definition (Name + Schema + Description).
- `pinned_at`: Timestamp and policy version when this tool was anchored.

### 4. Tamper-Evidence (Manifests)
Runs are bundled into **Evidence Manifests**.
- A manifest is a signed DSSE (Dead Simple Signing Envelope) containing a list of Event IDs.
- Future support for **Sigstore** to provide keyless signing based on OIDC identities.

## Consequences
- Independent auditors can verify the integrity of the audit trail without trusting the Assay binary itself.
- Policy drift is instantly detectable by comparing `decision_id` across runs.
- Slightly higher CPU overhead during execution for Blake3 hashing.
