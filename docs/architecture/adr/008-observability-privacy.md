# ADR 008: Observability & Privacy by Default (SOTA 2026)

**Status:** Accepted
**Date:** 2026-02-02
**Context:** GenAI observability (OpenTelemetry) and Privacy requirements for strict enterprise environments.

## Decision

We are adopting a "Bleeding Edge 2026" posture for Assay's telemetry and privacy system. This decision enforces strict defaults, "OpenClaw" guardrails, and cryptographic reference patterns to prevent data leakage while maintaining high-fidelity observability.

### 1. GenAI Semantic Conventions (Versioned & Pinned)
*   **Decision:** Pin GenAI Semantic Conventions to version **`1.28.0`** (Development).
*   **Enforcement:**
    *   Use a `GenAiSemConv` trait to abstract attribute keys, mapped strictly to v1.28.0.
    *   **Self-Describing:** Emit `assay.semconv.genai = "1.28.0"` as a resource/span attribute to enable future schema migrations.
*   **Rationale:** GenAI attributes change frequently. Version pinning prevents drift.

### 2. Privacy by Default (Explicit & Testable)
*   **Decision:** `otel.capture_mode` MUST default to `Off`.
*   **Modes:**
    *   `Off`: No prompt/response content is ever emitted.
    *   `BlobRef` (Recommended): Payloads are uploaded to a secured Blob Store (BYOS). Only the opaque, non-guessable `blob_ref` is emitted.
    *   `RedactedInline` (Legacy/Debug): Payloads are scrubbed (Regex/JSON) and emitted inline. **Requires explicit opt-in.**
*   **Fail-Closed:** If `RedactedInline` is enabled but no redaction policies are defined, the system MUST fail startup or force-downgrade to `BlobRef`.

### 3. BlobRef Semantics (Leakage Prevention)
To prevent the "Reference" itself from becoming a leak vector:
*   **Content-Addressed:** ID = `sha256(jcs(payload) + salt)`.
*   **Opaque:** The span NEVER contains the storage URL, only the ID (`assay.blob.ref`).
*   **Attributes:**
    *   `assay.blob.ref`: The opaque hash.
    *   `assay.blob.kind`: `"prompt" | "completion" | "tool_io"`
    *   `assay.blob.redaction`: `"none" | "policy:v1" | "deny"`

### 4. Telemetry Surface Guardrails ("OpenClaw" Defense)
If `capture_mode` is NOT `Off`, we enforce strict transport security to prevent exfiltration to attacker-controlled listeners.

1.  **Transport Security:**
    *   **TLS Mandatory:** `OTEL_EXPORTER_OTLP_ENDPOINT` must start with `https://`.
    *   **Anti-Bypass:** Validate DNS resolution at startup (no private -> public jumps).
2.  **Endpoint Allowlist:**
    *   `exporter.allowlist` config MUST be present and match the endpoint.
3.  **Localhost Binding (Debug Surface):**
    *   **Deny Localhost:** Reject `localhost`/`127.0.0.1` endpoints unless `exporter.allow_localhost = true` is explicitly set.
    *   Why: Prevents "OpenClaw" incidents where debug collectors bind publicly or to unprivileged local ports.

### 5. Low-Cardinality Metrics (Hard Shield)
*   **Decision:** Strictly enforce low-cardinality for all metrics.
*   **Mechanism:** `MetricRegistry` contains a `FORBIDDEN_LABELS` set (`trace_id`, `user_id`, `prompt_hash`).
*   **Enforcement:**
    *   **Debug/Test:** Panic (Fail-Closed).
    *   **Release:** Log Error and Drop Dimension.

### 6. Defense in Depth
*   **Collector-Side Redaction:** We formally recommend an OTel Collector Redaction Processor as the "last line of defense" (using hashing/pseudonymization) in the deployment pipeline, separate from the application logic.

## Verification Plan

### A. Golden Snapshot (Robust)
Instead of comparing raw JSON (fragile):
1.  **Normalize:** Sort keys, strip timestamps/IDs, remove non-owned resource attrs (`process.pid`, `host.name`).
2.  **Compare:** Match against `tests/fixtures/otel/v1_28_0_golden.json`.

### B. Invariant Contract Tests
Independent of the snapshot:
*   `capture_mode: Off` => Assert NO `gen_ai.prompt` or content fields.
*   `capture_mode: BlobRef` => Assert `assay.blob.ref` present, `gen_ai.prompt` ABSENT.
*   `capture_mode: RedactedInline` + No Policy => Assert Startup Failure.

## Roadmap Alignment
*   **Q3 2026 (P1):** E8 GenAI SemConv + E5 Privacy Defaults + BlobRef basics.
*   **Q2/Q3 2026 (P2):** Advanced Hardening (DNS Anti-Bypass, Collector Templates).
