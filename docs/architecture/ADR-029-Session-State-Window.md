# ADR-029: Session & State Window Contract (MCP Governance)

## Status
Accepted (March 2026; implemented on `main` via PRs #569, #574, and #576)

## Context
Assay enforces deterministic governance for MCP tool routes. Several experiment lines demonstrated that stateful invariants (sequence/state) generalize beyond lexical checks, including tool-hopping and cross-session delayed sink attempts.

To keep the product surface audit-grade, we need a frozen contract for:
- how we identify a "session"
- how we define a "state window" across sessions
- what we store (privacy/retention defaults)
- how we produce deterministic snapshot identifiers

This ADR freezes the contract only. It does not add storage backends, enforcement modes, or workflows.

## Decision
Introduce `session_state_window_v1` as an informational contract describing:
1) **Session key**: stable identifiers that partition MCP activity into sessions.
2) **State window**: a bounded time/run window where prior actions remain relevant.
3) **Privacy/retention defaults**: no raw bodies, no prompt/tool argument storage by default.
4) **Deterministic snapshot id**: content-addressed state identifiers derived from a canonical snapshot payload.

### 1) Session key contract (frozen)
A session is identified by the tuple:

- `event_source` (string) — normalized origin, e.g. `assay://…`
- `server_id` (string) — MCP server label / wrapped target identity
- `session_id` (string) — generated per run, unique within `(event_source, server_id)`

Notes:
- `session_id` MUST be opaque (no embedded PII, no timestamps required).
- `session_id` MUST be stable for the duration of a wrapped MCP process.
- Any cross-session state references MUST use `state_snapshot_id` (not raw content).

### 2) State window contract (frozen)
A state window is a bounded relevance horizon used by governance logic:

- `window_kind`: `session` | `cross_session_decay`
- For `session`: relevance lasts only within the session.
- For `cross_session_decay`: relevance spans subsequent sessions, bounded by:
  - `decay_runs` (integer >= 1): number of subsequent sessions where the window remains active.

No other decay semantics (time-based TTL, hybrid windows) are defined in v1.

### 3) Privacy/retention defaults (frozen)
Default data handling rules:
- Raw tool arguments, raw document bodies, raw prompts are **not** stored in state snapshots.
- State snapshots contain only:
  - tool names
  - tool classes (if known)
  - decision codes/reasons
  - hashes/refs (content-addressed ids), never the raw payload

Optional attachments MAY exist in the broader system, but are explicitly out of scope for this ADR.

### 4) Deterministic snapshot id (frozen)
`state_snapshot_id` is content-addressed:

- `snapshot_canonical_json` is produced using canonical JSON (key-order independent).
- `state_snapshot_id = "sha256:" + hex(sha256(snapshot_canonical_json_bytes))`

This ADR freezes:
- snapshot id MUST be derived from the canonical snapshot payload
- snapshot id MUST be stable across runs when snapshot content is equal
- snapshot id MUST NOT depend on local paths, timestamps, hostnames, or random values.

## Schema
A JSON schema `session_state_window_v1` defines the report/snapshot envelope for:
- session key fields
- state window parameters
- snapshot id + canonicalization fields
- privacy assertions (what is intentionally not present)

## Consequences
### Positive
- Makes session/state semantics explicit and auditable.
- Enables deterministic replay and cross-session experiments without implying storage/enforcement.
- Provides a stable interface for future runtime implementations.

### Negative
- Some future use cases may need time-based TTL or richer provenance.
- v1 intentionally does not define backend storage or enforcement integration.

## Out of scope (explicit)
- Any runtime/backend implementation
- Any new policy DSL
- Any workflow / CI gating changes
- Any taint tracking or dataflow labeling
- Any persistence format beyond this informational contract

## Acceptance criteria (for this A-slice)
- Freeze slice introduced the ADR as Proposed; the contract is now implemented on `main`.
- `session_state_window_v1` schema exists.
- Reviewer gate enforces allowlist-only scope + workflow-ban and validates schema parses.
