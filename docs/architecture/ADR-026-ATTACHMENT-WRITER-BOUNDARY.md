# ADR-026 AttachmentWriter Host Boundary (E2A)

## Intent
Freeze the host-side policy boundary for raw payload preservation used by protocol adapters.

The adapter contract remains minimal, but host implementations of `AttachmentWriter` must enforce a consistent safety policy before runtime rollout.

## Scope
In-scope:
- host-enforced payload size caps
- media-type allowlist / validation
- explicit redaction boundary ownership
- error taxonomy for attachment persistence failures

Out-of-scope:
- runtime implementation of the host writer (E2B)
- workflow changes
- release-lane integration changes

## Host policy contract (v1)
A production `AttachmentWriter` implementation must enforce:
- a hard maximum payload size before persistence
- media-type validation against an explicit allowlist or contract rule
- no direct adapter-managed filesystem writes
- no raw payload contents in logs

## Redaction boundary
Adapters do not perform redaction.

Redaction, rejection, or secret-handling policy is owned by the host layer implementing `AttachmentWriter`.

## Error taxonomy
The host writer contract must distinguish at least:
- measurement/contract failures
  - oversize payload
  - unsupported or invalid media type
  - invalid persistence contract inputs
- infrastructure failures
  - storage write failure
  - unavailable attachment backend

## Required output shape
Successful writes must produce a stable digest-backed reference containing:
- `sha256`
- `size_bytes`
- `media_type`

## Non-goals
- no adapter mapping behavior changes in this freeze slice
- no workflow changes
- no crates.io publication changes
