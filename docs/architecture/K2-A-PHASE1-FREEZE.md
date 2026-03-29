# K2-A — Phase 1 formal freeze

**Status:** Frozen for `K2-A` Phase 1 on `main`; not implemented, not released.
**Parent:** [PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md](PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md).
**Prep input:** [K2-A-PHASE1-FREEZE-PREP.md](K2-A-PHASE1-FREEZE-PREP.md).
**Repo snapshot:** Current `main` has bounded `G3` authorization **context** on supported
`assay.tool.decision` evidence, but it does **not** yet have a first-class MCP
authorization-discovery seam in canonical evidence or MCP adapter/server output. This freeze is
therefore **pre-implementation**: it locks the honest source classes, semantic ceiling, and review
gates before any runtime code is allowed to promote a new seam.

## Contract honesty (product / review)

**`K2-A` v1 is MCP authorization-discovery visibility evidence only.** It may only say that a
bounded authorization-discovery surface was **visible**, **advertised**, or **observed** on a
supported MCP code path. It is **not** an auth-success signal, not a compliance surface, not an
issuer-trust signal, and not an enterprise-readiness claim.

## 1. Definitive decision: repo-reality-first, one seam, no pack

`K2-A` Phase 1 is frozen as:

- **one bounded MCP authorization-discovery seam**
- **repo-reality first**
- **adapter/server/proxy emitted evidence only**
- **no pack in the same slice**
- **no reuse of `G3` authorization-context semantics**

This freeze does **not** pre-commit a final JSON field name. It freezes the semantic surface and
allowed inputs first. Any later implementation must keep the seam singular and typed.

## 2. Hard semantic ceiling

`K2-A` may only imply that:

- authorization-discovery information was visible on a supported MCP path
- the discovery source class was bounded and observed
- a protected-resource metadata surface was visible
- an `authorization_servers` surface was visible
- a scope challenge was visible

`K2-A` must **not** imply:

- authorization succeeded
- required scopes were sufficient
- the discovered authorization server is trusted
- the server is secure or compliant in the broad sense
- a client is correctly configured
- OAuth/OIDC discovery was complete end-to-end

## 3. Allowed positive source classes (v1 freeze)

These source classes are the **only** classes that a future `K2-A` implementation may promote in
v1, and only when they are observed on a supported MCP runtime/discovery path with exact source
provenance.

| Source class | May imply | Must not imply |
|--------------|-----------|----------------|
| `WWW-Authenticate` `resource_metadata` on a supported MCP `401` path | a bounded authorization-discovery surface was visible on the response path | auth failure correctness, scope sufficiency, server trust |
| Protected Resource Metadata document fetched from a frozen supported source URL | protected-resource metadata was visible as typed discovery input | that the document was valid, trustworthy, or complete |
| Typed `authorization_servers` list inside protected-resource metadata | authorization-server locations were visible | issuer trust, server correctness, enterprise-readiness |
| Typed scope challenge visible in `WWW-Authenticate` | a scope challenge was visible | that the scope requirement was correct or sufficient |

## 4. Explicit v1 non-promotion rules

`K2-A` Phase 1 must reject promotion from:

- bearer tokens, access tokens, refresh tokens, client secrets, or credential-like headers
- opaque auth blobs or copied header strings without typed source provenance
- static config or docs that were not observed on a supported MCP discovery/runtime path
- generic logs or trace text
- policy decisions alone
- `G3` authorization-context fields (`auth_scheme`, `auth_issuer`, `principal`)
- inferred issuer trust or inferred compliance
- OAuth AS metadata / OIDC discovery fetched outside the bounded v1 source classes

## 5. Repo-reality gate before implementation

No `K2-A` implementation PR is honest unless it first shows:

1. the exact shipped MCP code path that observed the discovery source
2. the exact source path for the promoted typed field(s)
3. precedence rules if multiple discovery sources are visible
4. one emitted JSON example on a supported path
5. a negative matrix proving non-promotion for secret-bearing, blob-like, config-only, and
   `G3`-only inputs

If those inputs are missing, the correct next step is more discovery or adapter/server plumbing,
not seam promotion.

## 6. Review gates (hard fail)

Any future `K2-A` implementation must hard-fail review if it:

- promotes anything from outside the source classes in §3
- reuses `G3` fields as if they were discovery evidence
- promotes authorization-discovery from static config without observed runtime/discovery provenance
- emits credentials or secret-bearing material
- turns visibility into auth correctness, server trust, or compliance language
- widens the work into a pack, engine bump, or broader Trust Basis / Trust Card expansion

## 7. Status meaning

This freeze means:

- `K2` is the planned next bounded evidence wave
- `K2-A` Phase 1 now has a formal pre-implementation contract
- implementation is allowed only inside the guardrails above

This freeze does **not** mean:

- a `K2-A` seam is already implemented
- a pack should follow automatically
- field names are final
- `G3` or existing MCP decision evidence is enough by itself

## References

- [PLAN — K2 MCP Authorization-Discovery Evidence (Q2 2026)](./PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md)
- [K2-A — Phase 1 freeze prep](./K2-A-PHASE1-FREEZE-PREP.md)
- [RFC-005 Trust Compiler MVP](./RFC-005-trust-compiler-mvp-2026q2.md)
- [Trust Compiler Audit Matrix](./AUDIT-MATRIX-TRUST-COMPILER-2026-03-26.md)
