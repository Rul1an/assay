# Split inventory: wave-g2-delegation-context-signal-step1

## Goal

Surface explicit delegation context on existing `assay.tool.decision` evidence
for one supported flow, without adding a new event type or broader delegation
validation semantics.

## Supported flow

- MCP/tool-call requests that carry explicit `_meta.delegation` metadata
- required carrier field:
  - `delegated_from`
- optional additive carrier field:
  - `delegation_depth`

## In scope

- additively extend existing decision evidence with:
  - `delegated_from`
  - `delegation_depth`
- parse explicit `_meta.delegation` request metadata
- flow those fields through:
  - `PolicyMatchMetadata`
  - `ToolMatchMetadata`
  - `PolicyDecisionEventContext`
  - `DecisionData`
- add targeted positive/negative tests
- small docs update for evidence contract and OWASP mapping
- reviewer gate and wave artifacts

## Out of scope

- new event types
- `actor_chain`
- `inherited_scopes`
- delegation engine or chain validation
- cryptographic delegation proof
- reference or temporal delegation semantics
- pack YAML changes
- runtime behavior changes outside signal emission

## Truth freeze

- emit only from explicit `_meta.delegation`
- `delegation_depth` is reported, never inferred
- direct/non-delegated flows stay unchanged
- loose human-readable hints are not promoted into typed evidence
