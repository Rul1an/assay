# ADR-026 Parser Hardening Boundary (E4A)

## Intent
Freeze the parser-hardening boundary for ADR-026 protocol adapters before adding
property tests or deeper runtime guards.

The goal is to treat ACP and A2A inputs as attack surface, not only as trusted
fixtures, while keeping the blast radius limited to adapter parsing and
conformance behavior.

## Scope
In-scope:
- parser threat model for ACP and A2A adapters
- hard caps policy for payload bytes, JSON depth, and array length
- strict vs lenient rules for malformed inputs and parser guardrails
- test requirements for property-based hardening in E4B

Out-of-scope:
- workflow changes
- crates.io publication changes
- changes to adapter mapping semantics outside parser guardrails
- adapter registry / Wasm plugin work

## Threat model
Adapters must assume upstream inputs may be adversarial.

Minimum adversarial cases to harden against:
- deeply nested JSON intended to exhaust recursion or stack depth
- very large arrays intended to amplify parse or normalization cost
- oversized payload bytes intended to exhaust memory or storage policy
- malformed JSON and invalid UTF-8 byte sequences
- unrecognized fields or enum-like values used to trigger lossy fallback paths

## Hard caps contract (v1)
The following caps are frozen for implementation in E4B:
- `max_payload_bytes`: enforced before deep parsing begins
- `max_json_depth`: enforced while traversing decoded JSON values
- `max_array_length`: enforced for any adapter-consumed array

Contract requirements:
- cap violations are measurement/contract failures
- malformed JSON is a measurement/contract failure in all modes
- invalid UTF-8 input is a measurement/contract failure in all modes
- lenient mode may preserve semantically lossy inputs, but must not bypass
  structural parser failures or cap violations
- no silent truncation of arrays, objects, or bytes is allowed

## Enforcement locations
E4B must make enforcement locations explicit:
- payload byte caps at adapter ingress before deep parsing
- JSON depth and array length caps inside shared or adapter-local parse helpers
- conformance tests proving the caps are active for ACP and A2A

## Property test requirements (E4B)
Minimum hardening evidence required in E4B:
- at least one property/proptest per adapter exercising randomized unknown-field
  placement or object key ordering
- at least one cap test for deep nesting
- at least one cap test for large arrays
- at least one malformed byte/invalid UTF-8 test path

## Strict vs lenient boundary
Lenient mode remains valid only for semantic translation loss.

Lenient mode must not:
- accept malformed JSON
- accept invalid UTF-8 input
- accept cap violations for payload bytes, depth, or array length

## Non-goals
- no change to canonical event digest rules frozen in E3A/E3B
- no change to AttachmentWriter host policy frozen in E2A/E2B
- no change to release-lane behavior
