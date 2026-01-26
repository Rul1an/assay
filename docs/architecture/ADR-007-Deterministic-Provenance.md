# ADR-007: Deterministic Identification & Provenance

## Status
**Adopted** (Q1 2026 Strategy)

## Context
Evidence is only valuable if it is reproducible and verifiable ("same run = same IDs"). Random UUIDs break reproducibility and make deduplication difficult. We need a "content-addressed" identification scheme.

## Decision
Assay will implement a strict deterministic identification scheme based on **RFC 8785 (JSON Canonicalization Scheme)** and **Blake3/SHA-256** hashing.

### 1. Canonicalization (RFC 8785)
Before any hashing or signing, all JSON payloads MUST be canonicalized using **JCS**:
1.  **Keys Sorted**: Lexicographically.
2.  **Whitespace Removed**: No insigificant whitespace.
3.  **UTF-8**: Strict encoding.
4.  **Numbers**: IEEE 754 representation normalization (e.g. `1.0` -> `1`).

### 2. Event ID (Content-Addressed)
The `event_id` is derived from the canonical hash of the event content *excluding* the ID itself and variable request-time fields (like ingestion timestamps).

`event_input = canonical_json({ schema_version, type, run, producer, policy_ref, payload })`

`event_id = "sha256:" + hex(sha256(event_input))`

**Invariant**: Re-running the same command with the same inputs, environment, and policy MUST produce the exact same `event_id` sequence.

### 3. Run ID (Stable Context)
To allow "Golden Test" comparison of runs, the `run_id` can be generated in two modes:

1.  **Strict/Replay Mode**: Derived from input context.
    `run_id = "run_" + base64url(sha256(repo_root + policy_hash + command + strict_env_hash))`
2.  **Live Mode**: Time-ordered UUID v7 (for efficient DB indexing).

The export format will preserve whichever ID was used at runtime.

### 4. Evidence Manifests (Bundles)
An "Evidence Bundle" is the unit of export/transfer.

**Format**:
1.  **`events.ndjson`**: The stream of canonical event records.
2.  **`manifest.json`**: The integrity root.

```json
{
  "schema_version": 1,
  "bundle_id": "sha256:...",
  "producer": { "name": "assay", "version": "...", "git": "..." },
  "run_id": "...",
  "event_count": 42,
  "run_root": "sha256:...",
  "files": { "events": "events.ndjson" }
}
```

**Run Root Calculation**:
`run_root = sha256( concatenate( event_id bytes for all events in sequence ) )`

This creates a lightweight **Hash Chain** (Merkle sequence) that proves the integrity and order of the event stream.

## Consequences
- **Verifiability**: Any third party can take the `events.ndjson`, re-compute JCS hashes, and verify they match `event_id` and `run_root`.
- **Deduplication**: Identical runs produce identical IDs, enabling efficient storage.
- **Performance**: Canonicalization adds CPU overhead (mitigated by Rust `serde_jcs` performance).
