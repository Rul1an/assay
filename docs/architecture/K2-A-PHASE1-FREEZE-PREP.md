# K2-A — Phase 1 freeze prep

**Status:** Historical prep input; `K2-A` now has a formal pre-implementation freeze in [K2-A-PHASE1-FREEZE.md](K2-A-PHASE1-FREEZE.md), but no implementation or release is shipped yet.
**Parent:** [PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md](PLAN-K2-MCP-AUTHORIZATION-DISCOVERY-EVIDENCE-2026q2.md).
**Purpose:** Record the exact discovery questions, candidate sources, and hard negatives that were
required before the real `K2-A` freeze could be written. It now remains as supporting input, not
the active contract document.

## 1. Contract honesty

`K2-A` v1 may only claim that a bounded MCP authorization-discovery surface is **visible**,
**advertised**, or **observed**.

It must **not** imply:

- that authorization succeeded
- that a scope challenge was correct
- that the discovered authorization server is trusted
- that the server is compliant or secure in the broad sense
- that the flow is enterprise-ready by default

## 2. Candidate source classes to audit

These are candidate source classes for discovery, not frozen positive rules.

| Candidate source class | Why it matters | Must stay bounded |
|------------------------|----------------|-------------------|
| `WWW-Authenticate` `resource_metadata` | Direct authorization-discovery surface on `401` responses | Visibility only; do not imply success or scope sufficiency |
| Protected Resource Metadata document | Canonical location for `authorization_servers` and related metadata | Promote only typed, allowlisted fields |
| `authorization_servers` list | Bounded server-location surface | Do not imply server trust or correctness |
| Scope challenge in `WWW-Authenticate` | Strong enterprise-review signal for requested scopes | Do not imply requested scope correctness |
| OAuth AS metadata / OIDC discovery fetched from frozen source URLs | Possible bounded follow-on source if already observable in current code paths | Do not widen beyond one seam without explicit freeze |

## 3. Hard negatives to preserve

The eventual freeze must explicitly reject promotion from:

- bearer tokens or other credentials
- opaque auth blobs
- static config that was not observed in a supported discovery/runtime path
- unstructured logs
- policy decisions alone
- inferred issuer trust
- generic metadata copies without source provenance

## 4. Mandatory discovery questions

Before `K2-A` can freeze, maintainers need concrete answers for:

1. Which shipped MCP code paths can observe authorization-discovery today?
2. Where exactly are `resource_metadata`, protected-resource metadata, and `authorization_servers`
   visible in those paths?
3. Which discovery source should win if multiple sources are present?
4. Which parts are typed enough for canonical evidence, and which must stay out?
5. Which source classes are honest only as visibility flags?
6. Which candidate fields would create secret leakage or correctness theater if promoted?

## 5. Minimum freeze inputs

The executable freeze should not be written until all of the following exist:

- per-source mapping table with exact code-path provenance
- precedence table for discovery sources
- emitted JSON example on a supported path
- negative matrix for non-promoting inputs
- explicit product-language guardrails

## 6. Review guardrails for the prep itself

The prep should hard-fail review if it:

- pre-commits field names as if they were already frozen
- treats spec importance as equivalent to repo-emitted evidence
- sneaks in pack or engine assumptions
- collapses discovery visibility into auth validation
- normalizes secrets into any candidate evidence shape
