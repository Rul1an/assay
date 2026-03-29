# PLAN — K2 MCP Authorization-Discovery Evidence (2026 Q2)

- **Current status:** Planned next bounded trust-compiler wave after `K1-A` stabilization on `main`; no implementation or freeze is shipped in this plan.
- **Date:** 2026-03-29
- **Owner:** Evidence / Product
- **Inputs:** [ROADMAP](../ROADMAP.md), [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md), [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md), [K1-A — Phase 1 formal freeze](./K1-A-PHASE1-FREEZE.md), [Trust Compiler Audit Matrix](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md), [MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-05/basic/authorization)
- **Scope (this PR):** Formalize the next wave choice and its guardrails only. No MCP adapter/runtime code, no pack YAML, no engine work, and no Trust Basis / Trust Card expansion.

## 1. Why this plan exists

`K1-A` gave Assay its first bounded A2A handoff / delegation-route seam. The next product move
for traction should not be "more internal engine" or "another pack by default"; it should be the
next enterprise-relevant protocol surface that still needs **first-class evidence** before any
claim productization.

That next surface is:

- **`K2` — MCP authorization-discovery evidence**

`K2` continues the same product discipline as `K1`:

- evidence-first
- CI-native
- one bounded seam
- no pack in the same slice
- no trust-score or correctness theater

## 2. Goal (one sentence)

Add a **first-class, bounded canonical evidence seam** for **MCP authorization-discovery
visibility**, grounded in supported MCP authorization flows and kept strictly below correctness,
authorization-success, or trust claims.

## 3. Why `K2` now

### 3.1 Why this is the strongest next enterprise wedge

After `K1-A`, the sharpest next gap is not broader A2A semantics; it is **authorization-discovery
visibility** on the MCP side:

- where did the server expose authorization-discovery metadata?
- was a protected-resource metadata document visible?
- were `authorization_servers` visible?
- did the flow expose an explicit scope challenge?

Those questions matter for CI-native governance and auditability, but they are still **evidence**
questions first.

### 3.2 Why this is not a pack-first move

Even though MCP authorization-discovery is strategically important, Assay should not jump straight
to a pack or a stronger claim. A pack is only honest after the underlying seam is real and bounded
in canonical evidence.

### 3.3 Why this is not “more G3”

`G3` made **authorization context** visible on supported `assay.tool.decision` evidence. `K2` is a
different surface:

- `G3`: what auth context was visible on a decision path
- `K2`: what authorization-discovery surface the MCP server advertised or exposed

That distinction must stay explicit; `K2` must not silently reuse `G3` semantics.

## 4. Product framing

### In scope

- one bounded MCP **authorization-discovery** seam
- evidence emitted from supported MCP discovery / authorization flows
- visibility of protected-resource metadata and authorization-server discovery surfaces
- CI-native evidence and policy-review outputs
- exact negatives for blob-like, config-only, or secret-bearing inputs

### Out of scope

- any new pack in the `K2` wave
- any engine bump
- token validation, auth success, scope correctness, or issuer trust
- claims that a server is compliant, secure, or enterprise-ready
- reusing G3 authorization context as if it were discovery evidence
- Trust Basis or Trust Card expansion in the same slice
- broad OTel/OpenInference schema work in the same slice

## 5. Hard language contract

`K2` may only claim that authorization-discovery information is **visible**, **advertised**, or
**observed** in bounded evidence.

`K2` must **not** imply:

- the server is correctly secured
- the authorization server is trusted
- the configured flow is valid
- the required scopes were sufficient
- a request was authorized successfully
- the server is compliant with the full MCP auth model

## 6. Working v1 seam hypothesis

The likely outcome is **one small typed subobject** or equivalent bounded field set in canonical MCP
evidence. Exact field names are **not frozen in this PLAN**.

This PLAN only freezes the shape discipline:

- one seam
- one meaning
- typed beats blob
- observed discovery beats auth-success claims

Illustrative questions the seam may answer later:

- was protected-resource metadata visible at all?
- were `authorization_servers` visible?
- did the discovery source come from a `WWW-Authenticate` challenge, a well-known URI, or another
  frozen supported source?
- was a bounded scope challenge visible?

These examples are illustrative discovery directions, not provisional field commitments.

## 7. `K2-A` freeze-prep requirements

Before any implementation PR, `K2` must produce a freeze-ready discovery record that answers:

1. Which supported MCP code paths can actually observe authorization-discovery surfaces today?
2. Which sources are typed and stable enough for canonical promotion?
3. Which signals are only honest as visibility flags and not stronger claims?
4. What is the smallest honest seam?
5. Which inputs must **not** become typed authorization-discovery evidence?

Minimum artifacts required before freeze:

- per-source mapping table
- precedence rules when multiple discovery sources exist
- representative emitted JSON example
- explicit negative matrix for false positives and secret-bearing inputs
- hard may / must-not-imply language

That prep path is recorded in [K2-A — Phase 1 freeze prep](./K2-A-PHASE1-FREEZE-PREP.md).

## 8. Implementation gates (future, not this PR)

Any future `K2` implementation slice should hard-fail review if it:

- turns discovery visibility into authorization correctness
- promotes static config or docs as runtime-observed evidence without an explicit source rule
- promotes raw tokens, bearer headers, or other secrets into evidence
- silently reuses `G3` auth-context fields for discovery semantics
- sneaks in a pack, engine bump, or Trust Card expansion in the same wave
- widens the seam beyond one bounded authorization-discovery surface

## 9. Acceptance for `K2-A`

`K2-A` should only count as shipped if all of the following hold:

1. Canonical emitted MCP evidence gains one bounded authorization-discovery seam.
2. The seam is documented with exact source paths and bounded meaning.
3. Tests show the seam is **not** promoted from loose, static, or secret-bearing inputs.
4. Product language stays at **visible / advertised / observed**, not **valid / compliant / trusted**.
5. No pack or broader trust artifact is shipped in the same wave.

## 10. What happens after `K2`

Only after `K2` evidence is real should maintainers revisit whether:

- an MCP authorization-discovery pack is honest
- any Trust Basis / Trust Card expansion is justified
- an OTel/OpenInference export mapping should be widened for this seam

No downstream pack or trust-surface follow-up should be assumed as part of `K2` closure.

## References

- [ROADMAP](../ROADMAP.md)
- [RFC-005](./RFC-005-trust-compiler-mvp-2026q2.md)
- [PLAN — K1](./PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md)
- [K2-A — Phase 1 freeze prep](./K2-A-PHASE1-FREEZE-PREP.md)
- [Trust Compiler Audit Matrix](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md)
- [MCP Authorization](https://modelcontextprotocol.io/specification/2025-11-05/basic/authorization)
