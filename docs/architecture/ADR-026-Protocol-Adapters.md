# ADR-026: Protocol Adapters (Adapter-First Strategy)

## Status

Proposed (February 2026)

## Context

Assay's open-core value is protocol-agnostic governance with deterministic, verifiable evidence. External agent protocols such as ACP, A2A, and related ecosystem formats emit protocol-specific payloads and lifecycle models, not canonical Assay `EvidenceEvent` records.

Without a controlled adapter layer, protocol adoption creates two problems:
- Protocol-specific logic leaks into the evidence core.
- Fast-moving protocol revisions create drift between raw protocol payloads and Assay's canonical evidence model.

A dedicated adapter layer allows Assay to ingest protocol payloads, preserve audit-grade provenance, and keep the evidence core stable.

## Decision

We adopt an adapter-first strategy:
- Introduce a shared adapter API in a central crate such as `assay-adapter-api`.
- Implement protocol-specific adapters as separate crates such as `assay-adapter-acp` and later `assay-adapter-a2a`.
- Keep adapter crates in open core because they are interoperability infrastructure, not enterprise workflow features.
- Preserve raw protocol payloads via host-provided attachment writing and stable references instead of direct filesystem writes from adapters.
- Freeze standalone distribution separately: current adapter crates remain workspace-internal OSS crates and are not published to crates.io until a dedicated distribution slice lands (see `ADR-026-Adapter-Distribution-Policy.md`).

### v1 execution model

Adapters are native Rust crates linked into the workspace.

### v2 execution model (deferred)

The same API may later be hosted in sandboxed Wasm modules, but Wasm/plugin transport is explicitly deferred until the native contract is proven stable.

## Adapter API Contract (v1)

Each adapter must implement the following contract surface:
- `protocol() -> ProtocolDescriptor`
- `capabilities() -> AdapterCapabilities`
- `convert(input, options) -> Result<Vec<EvidenceEvent>, AdapterError>`
- `lossiness() -> LossinessReport` on each emitted output unit or batch result

### Required protocol metadata

`protocol()` must expose:
- `name`
- `spec_version`
- `schema_id`
- `spec_url`

### Required lossiness metadata

Each conversion result must make loss explicit via:
- `lossiness_level`: `none | low | high`
- `unmapped_fields_count`
- `raw_payload_ref`: digest-backed reference to preserved source payload

## Strictness Modes

Adapters must support at least two conversion modes via `ConvertOptions`:
- `strict`: fail on malformed or unmappable critical protocol data
- `lenient`: emit evidence plus explicit lossiness metadata and preserved raw payload reference

Normative behavior:
- `strict` contract failures exit with measurement/contract failure semantics (`exit 2` in CLI/script harnesses).
- `lenient` mode must not silently drop critical data; it must surface lossiness and retain a raw payload reference.

## Raw Payload Preservation

Raw protocol payloads are preserved hash-first for auditability.

Normative rules:
- Adapters must not write directly to arbitrary filesystem paths.
- Raw payload persistence is performed through a host-provided attachment writer from the evidence/core layer.
- Emitted evidence carries only `raw_payload_ref { sha256, size, media_type }` plus any allowed redaction metadata.
- Canonicalization for payload hashing must be deterministic and stable for the adapter contract version.

## Versioning and Conformance

Each active adapter must ship with a strict conformance suite.

### Definition of Done

Conformance suites must include:
- at least `N` golden happy-path fixtures for the supported protocol version range
- at least one negative fixture per supported protocol version
- explicit expected outcome per fixture
- a determinism check proving identical input fixture -> identical output digest

Negative fixtures must cover at least one of:
- malformed packet
- missing required fields
- oversize payload
- invalid enum or discriminant

Expected outcomes must be explicit:
- `strict` mode: measurement/contract fail (`exit 2`)
- `lenient` mode: emit lossiness plus `raw_payload_ref`

### Upgrade policy

Each adapter crate must declare an explicit supported version range, for example `>=2.11 <3.0`, and document deprecation policy when upstream protocol revisions break mappings.

## Security Posture and Invariants

Adapters process untrusted protocol payloads. The following invariants are mandatory:
- payload size caps before deep parsing
- schema/shape validation before semantic mapping
- no implicit network access from adapter conversion paths
- no direct adapter-managed filesystem writes
- `unsafe` remains disallowed under workspace policy unless a future ADR explicitly carves out an exception

## Non-Goals

This ADR does not introduce:
- protocol-specific business policy enforcement in the evidence core
- dynamic plugin loading or Wasm adapter execution
- remote adapter registries
- workflow changes or CI rollout lanes
- enterprise-only middleware surfaces

## MVP Scope

The first implementation target is `assay-adapter-acp`.

Rationale:
- ACP is a high-value governance wedge for commerce/intent evidence.
- ACP creates immediate pressure for deterministic audit trails around checkout, intent, and authorization flows.
- A2A remains the strategic follow-up once the adapter API and conformance harness are proven.

## Execution Plan

### PR-A (freeze)
- freeze adapter API contract
- freeze conformance harness contract
- freeze ACP as initial implementation target
- no runtime mapping logic

### PR-B (implement)
- add `assay-adapter-acp`
- add happy-path and negative fixtures
- add strict/lenient determinism tests

### PR-C (closure)
- add checklist and review pack
- add index/runbook references for adapter entrypoints
- close the rollout loop with reviewer gates

## Acceptance Criteria

- ADR defines adapter API v1 contract and strictness semantics
- ADR defines hash-first raw payload preservation using host-provided attachment writing
- ADR includes explicit negative-fixture conformance requirements
- ADR freezes ACP as MVP and A2A as follow-up
- No workflow or runtime behavior changes are introduced in this slice

## Consequences

### Positive
- keeps the evidence core protocol-agnostic
- makes protocol drift explicit through conformance fixtures
- preserves audit-grade provenance for unmapped data
- opens protocol interoperability as open-core infrastructure

### Negative
- adds maintenance burden for evolving upstream protocols
- lossiness handling requires strong reviewer discipline
- adapter version ranges and fixtures become part of the compatibility surface

### Mitigations
- strict conformance suites
- explicit lossiness reporting
- host-controlled attachment writing
- freeze-first A/B/C rollout discipline
