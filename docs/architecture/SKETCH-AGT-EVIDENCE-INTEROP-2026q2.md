# Sketch: AGT Evidence Interop

Date: 2026-04-06
Status: v1 sketch only

## Purpose

This note sketches the smallest useful interop sample between Assay and the
Microsoft Agent Governance Toolkit (AGT).

It is intentionally narrow. It is not a roadmap commitment, a partnership
announcement, or a claim that Assay should absorb AGT concepts wholesale.

The goal is simpler:

- let AGT stay a runtime governance layer
- let Assay stay a CI-native evidence compiler
- test one honest handoff between them

## Current read

AGT looks strongest when it is doing runtime governance:

- policy enforcement
- identity and trust gating
- framework-facing integrations
- security scan and workflow surfaces

Assay looks strongest when it turns runtime artifacts into bounded,
reviewable evidence:

- evidence bundles
- Trust Basis
- Trust Card
- SARIF and CI-facing outputs

The overlap is real, but the products do not need to become each other.

## Recommended v1 seam

Use **AGT `mcp-trust-proxy` audit decisions** as the first interop surface.

Why this seam first:

- it is already small
- it matches Assay's MCP wedge
- it does not require Assay to import AGT trust semantics as truth
- it avoids the still-moving receipt verifier boundary
- it avoids turning SARIF import into the entire story

The relevant AGT shape today is the `AuthResult` audit log emitted by
`mcp-trust-proxy`, which currently includes:

- `allowed`
- `tool`
- `agent_did`
- `reason`
- `trust_score`
- `timestamp`

See:

- `packages/agentmesh-integrations/mcp-trust-proxy/mcp_trust_proxy/proxy.py`
- `packages/agentmesh-integrations/mcp-trust-proxy/tests/test_mcp_proxy.py`

## v1 sample flow

The first sample should be boring on purpose:

1. AGT runs `mcp-trust-proxy` in front of an MCP surface.
2. AGT writes a tiny frozen audit corpus with:
   - one allow decision
   - one deny decision
   - one malformed or import error case
3. Assay imports that corpus into a bundle as external runtime evidence.
4. Assay emits evidence artifacts without promoting AGT trust scores into
   Assay truth claims.

That is enough to prove whether the seam is real.

## Proposed frozen corpus

The corpus can start as three tiny JSON records:

```json
{"allowed":true,"tool":"web_search","agent_did":"did:mesh:agent-1","reason":"Authorized","trust_score":600,"timestamp":1712230000.0}
{"allowed":false,"tool":"shell_exec","agent_did":"did:mesh:agent-1","reason":"Tool 'shell_exec' is blocked by policy","trust_score":600,"timestamp":1712230001.0}
{"error":"malformed_record","raw":"{not-json}"}
```

The exact transport can be:

- NDJSON
- JSON array
- or a tiny exporter helper inside AGT examples

For v1, NDJSON is the simplest choice.

## Assay-side mapping

Assay should treat this as **external runtime policy evidence**, not as a new
source of trust truth.

Suggested mapping:

| AGT field | Assay treatment |
|---|---|
| `allowed` | observed runtime decision |
| `tool` | observed tool identifier |
| `agent_did` | observed actor identifier |
| `reason` | observed explanatory string |
| `timestamp` | observed external timestamp |
| `trust_score` | observed metadata only, never promoted |

Suggested imported event shape:

```json
{
  "kind": "external.runtime.policy_decision",
  "source": "agt:mcp-trust-proxy",
  "observed": {
    "allowed": false,
    "tool": "shell_exec",
    "agent_did": "did:mesh:agent-1",
    "reason": "Tool 'shell_exec' is blocked by policy",
    "timestamp": 1712230001.0,
    "trust_score": 600
  }
}
```

## Epistemology

This part matters more than the transport.

Assay should keep the imported AGT signal in the correct bucket:

- `allowed` and `reason` are observed runtime artifacts from AGT
- `trust_score` is observed metadata, not a trust fact
- Assay must not translate AGT scores into Assay trust tiers
- Assay must not imply that AGT policy adequacy was independently verified

In plain terms:

- AGT says what it enforced
- Assay says what it saw
- neither side should overclaim the other

## Why not signed receipts first

Signed receipts may become a good seam later, but they are not the cleanest
v1 for Assay right now.

Reasons:

- the verifier boundary is still moving
- exit code semantics are still being tightened
- receipt semantics are richer than the first sample needs
- the simplest proof of interop does not require cryptographic receipts yet

Receipts are better treated as a possible v2, after the smaller runtime-decision
seam is proven.

## Why not trust-score import

Importing AGT trust scores as if they were Assay-native trust claims would be a
category error.

Assay's strongest line today is:

- bounded claims
- explicit epistemology
- no primary score-first trust surface

So v1 should explicitly reject:

- score translation
- tier mapping
- AGT trust score as ground truth

## Success criteria

The v1 sample is successful if:

- a frozen AGT decision corpus imports cleanly into Assay
- the resulting bundle is deterministic
- Trust Basis and Trust Card do not overclaim imported AGT semantics
- the imported deny stays visible as runtime evidence
- malformed import cases fail clearly

The v1 sample is not trying to prove:

- full AGT support
- trust-score interoperability
- receipt interoperability
- deep framework integration

## Reasonable follow-up, if v1 lands

If the first sample is clean, the next most sensible follow-up is:

- AGT `security-scan` action or SARIF as a second imported surface

That would give Assay one runtime governance input and one CI scan input
without forcing either product into a broader merger story.

## External ask, if AGT maintainers engage

The best next ask from AGT is small:

- one tiny frozen `mcp-trust-proxy` audit corpus
- one note on the intended export format
- one example flow under `examples/` or `agentmesh-integrations/`

That is enough to move from repo-to-repo discussion into a real interop sample.
