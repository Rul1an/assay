# PLAN - P57 MCP Tunnel Observed-Facts Evidence (2026 Q2)

- **Date:** 2026-06-03
- **Owner:** Evidence / MCP Security
- **Status:** Planning
- **Scope (this PR):** Define the first Assay-adjacent evidence lane for MCP
  tunnel surfaces. This is a planning note only: no implementation, no public
  outreach, no schema freeze, and no claim that any tunnel provider currently
  exposes this exact artifact.

## 1. Why this plan exists

MCP tunnels are becoming a practical enterprise path for private MCP servers:
an operator runs a local or private-network client, the client keeps an
outbound connection to a hosted edge or control plane, and hosted agents can
reach the private MCP server without opening an inbound firewall rule.

That solves a reachability problem, but it creates a review problem:

> When a tool call crosses a tunnel, which facts did the tunnel actually
> observe, and which facts still belong to the MCP server, policy layer, tool,
> or application outcome?

Assay's adjacent role should be narrow:

- consume tunnel-observed facts as bounded external evidence;
- pair those facts with MCP decision/outcome records when a join exists;
- keep transport, authentication, policy, and tool outcome claims separate;
- avoid treating tunnel presence as proof of agent identity, authorization, or
  runtime truth.

This is not a tunnel product plan.

This is not an OpenAI, Anthropic, Tapinto, Cloudflare, ngrok, or gateway
integration plan.

This is not a network-security attestation plan.

This is a plan for the smallest honest **MCP tunnel observed-facts seam**.

## 2. External signals

Current public tunnel signals point in the same direction:

- Anthropic MCP tunnels position the tunnel as outbound-only access to private
  MCP servers, with tunnel transport, proxying, upstream OAuth, and shared
  responsibility kept as separate security layers. Source:
  <https://platform.claude.com/docs/en/agents-and-tools/mcp-tunnels/overview>.
- OpenAI `tunnel-client` exposes an operator-visible daemon with health,
  readiness, metrics, admin UI, redacted support bundle, control-plane
  polling, MCP forwarding, channel routing, OAuth diagnostics, and optional
  mTLS to the private MCP server. Source:
  <https://github.com/openai/tunnel-client>.
- Tapinto describes an MCP-aware tunnel whose edge parses JSON-RPC traffic and
  emits normalized inspector events for tool calls, resource reads, and prompt
  fetches. Source: <https://tapinto.dev/mcp-tunnel>.

Those signals do not define Assay scope. They motivate the same boundary:
tunnels can observe useful transport and MCP-routing facts, but those facts are
not enough to prove policy correctness, agent identity, tool result truth, or
application outcome.

Treat these as live external signals, not durable normative inputs. If a source
changes, the P57 artifact boundary should still stand: consume only the
smallest observed facts that can be made reviewable.

## 3. Hard positioning rule

Normative framing:

> P57 targets tunnel-observed transport and request-routing facts for MCP
> traffic, not tunnel trust, network security, OAuth correctness, policy
> correctness, issuer identity, or tool outcome truth.

That means:

- the tunnel is an observation point, not the truth source;
- the tunnel observation point can be bypassed or spoofed unless its own
  integrity is separately established;
- the MCP server remains the source of tool protocol behavior;
- policy decision records remain separate evidence;
- server execution records remain separate evidence;
- tool/application outcomes remain separate evidence;
- authentication metadata is observed context unless separately verified;
- tunnel uptime, routing, or successful forwarding does not prove the tool did
  what it was authorized to do.

Common anti-overclaim sentence:

> A tunnel can say "this request crossed this observed route"; it cannot by
> itself say "this agent was authorized" or "this tool outcome is true."

Stronger anti-forgery sentence:

> A tunnel artifact can report route facts observed by a specific observation
> point; it does not prove that the observation point was unbypassable, that
> upstream headers could not be spoofed, or that mediation happened unless a
> separate integrity or attestation layer establishes that.

## 4. Why not tunnel-security-first

The tempting first wedge is to talk about secure tunnels.

That would be the wrong Assay wedge.

Why:

- tunnel security depends on provider-specific control planes, keys,
  certificates, transport providers, deployment posture, and network policy;
- those properties are too broad for a small portable evidence seam;
- Assay would risk sounding like a tunnel verifier or security scanner;
- it would blur operator diagnostics with evidence semantics.

The cleaner first wedge is smaller:

- one bounded tunnel observation artifact;
- one request instance digest;
- one channel or route label;
- one upstream target label or digest;
- one optional MCP method/tool label when visible;
- one explicit non-claims section.

## 5. Why not auth-first

Tunnel deployments often preserve or forward MCP OAuth, bearer tokens, mTLS, or
enterprise identity context.

That is important, but it should not be the first P57 seam.

Why:

- authentication semantics differ across tunnel products and upstream MCP
  servers;
- a forwarded `Authorization` header is not the same as verified authorization;
- tunnel-layer credentials and MCP-server credentials answer different
  questions;
- auth-first would collide with policy-decision evidence and issuer-trust
  lanes.

P57 should allow auth context to be observed, redacted, or digested, but not
make auth success a tunnel-observed fact unless a separate verifier artifact
exists.

## 6. Why not inspector-event-first

MCP-aware tunnels and proxies may emit inspector events for:

- tool calls;
- resource reads;
- prompt fetches;
- JSON-RPC requests and responses;
- streaming notifications.

Those are valuable, but inspector events alone are still not enough.

Why:

- an inspector observes traffic shape, not policy correctness;
- an inspector may see request/response bytes without knowing redaction or
  application semantics;
- events can become too chatty for a first portable artifact;
- raw JSON-RPC capture increases privacy and payload-risk pressure.

The first P57 seam should be a compact observed-facts artifact. Inspector
streams can later map into that artifact or produce multiple artifacts, but the
first contract should stay small.

## 7. Recommended v1 seam

Use one frozen serialized artifact describing one tunneled MCP request
observation.

Working name:

```text
mcp_tunnel_observed_v0
```

The artifact should describe:

- the tunnel observation point;
- the bounded request instance;
- the route/channel used;
- the upstream target boundary;
- optional MCP method/tool/resource/prompt labels when visible;
- optional redacted/digest-only request envelope metadata;
- non-claims.

It should not include:

- raw arguments by default;
- raw tool results by default;
- bearer tokens or client certificates;
- full request/response payloads;
- policy decision bodies;
- issuer trust decisions;
- OAuth verification results;
- application outcome truth.

## 8. v1 artifact contract

### 8.1 Required fields

The first sample should require:

- `schema`
- `artifact_id`
- `observed_at`
- `provider_context`
- `tunnel`
- `request_instance`
- `route`
- `upstream`
- `visibility`
- `non_claims`

### 8.2 Optional fields

The first sample may include:

- `mcp`
- `auth_context`
- `control_plane`
- `inspector_event_refs`
- `evidence_refs`
- `notes`

### 8.3 Field boundaries

#### `provider_context`

This is descriptive context for the observation source, not a trust claim.

Suggested shape:

```json
{
  "provider": "example",
  "surface": "mcp_tunnel",
  "component": "tunnel-client",
  "component_version": "0.0.0"
}
```

#### `tunnel`

This records the tunnel instance observed by the source.

Suggested shape:

```json
{
  "tunnel_ref": "tunnel_...",
  "tunnel_ref_kind": "provider_id",
  "direction": "outbound_client_poll",
  "transport": "https_long_poll"
}
```

Allowed `direction` values for v0:

- `outbound_client_poll`
- `outbound_websocket`
- `outbound_cloudflared`
- `unknown`

Allowed `transport` values for v0:

- `https_long_poll`
- `websocket`
- `cloudflare_tunnel`
- `other`
- `unknown`

These are observed transport labels, not proof that the deployment is secure.
They are also not L3/L4 ground truth. For example, a tunnel client may observe
`cloudflare_tunnel`, `websocket`, or `https_long_poll` at the application layer
while the underlying peer set, datagram path, or QUIC behavior differs from
what a lower-level network observer would report.

#### `request_instance`

This identifies the bounded request observation.

Suggested shape:

```json
{
  "request_id": "optional-provider-request-id",
  "request_envelope_digest": "sha256:...",
  "request_envelope_canonicalization": "jcs:mcp_request_envelope.v1",
  "nonce": "optional-provider-nonce"
}
```

`request_envelope_digest` should be over a bounded projection, not raw payload
bytes. The v0 projection should include only:

- JSON-RPC version;
- method;
- id when present;
- params digest or params absent marker;
- `_meta` digest or `_meta` absent marker.

Raw params should be excluded by default. Tunnel-only route, channel, and
upstream labels should also stay outside `request_envelope_digest`; capture
them separately under `route` and `upstream`. That keeps the digest
request-specific enough to join with server-side execution or policy records
that never observed the tunnel route. If replay needs route context, reference
the route fields separately rather than folding them into the request envelope
digest.

#### `route`

This records the observed route/channel through the tunnel.

Suggested shape:

```json
{
  "channel": "main",
  "method": "tools/call",
  "path": "/mcp"
}
```

`channel` is a route label, not an actor identity.

#### `upstream`

This records the private MCP target boundary as observed by the tunnel client.

Suggested shape:

```json
{
  "target_ref": "local-stdio",
  "target_kind": "stdio",
  "target_digest": "sha256:..."
}
```

`target_ref` should be redacted or omitted when it includes sensitive hostnames,
paths, or internal topology.

#### `mcp`

This records MCP-level labels when visible.

Suggested shape:

```json
{
  "method": "tools/call",
  "tool_name": "deploy_service"
}
```

For `resources/read` or prompt operations, use source-specific fields such as
`resource_uri_digest` or `prompt_name` rather than forcing every operation into
`tool_name`.

#### `auth_context`

This records observed auth context without claiming auth correctness.

Suggested shape:

```json
{
  "authorization_header_visible": true,
  "authorization_header_stored": false,
  "authorization_header_digest": "sha256:...",
  "mcp_oauth_metadata_visible": true,
  "client_mtls_configured": false
}
```

These fields do not prove that OAuth, bearer-token, or mTLS authentication
succeeded. They only describe what the observation point could see or had
configured.

#### `visibility`

This section makes lossiness explicit.

Suggested shape:

```json
{
  "request_payload_mode": "digest_only",
  "response_payload_mode": "not_observed",
  "tool_result_visible": false,
  "policy_decision_visible": false,
  "raw_payload_retained": false
}
```

Allowed payload modes:

- `not_observed`
- `digest_only`
- `redacted_projection`
- `raw_retained`

`raw_retained` should not appear in the first public sample.

#### `evidence_refs`

This can point to separate evidence artifacts, for example:

- MCP policy decision evidence;
- SEP-2828-style server execution records;
- OAuth verification evidence;
- tunnel support bundle evidence;
- OpenTelemetry spans.

The references are joins, not embedded proof.

Suggested shape:

```json
[
  {
    "kind": "mcp.execution_record",
    "digest": "sha256:...",
    "relationship": "same_request_instance",
    "join_strength": "strong"
  }
]
```

`relationship=same_request_instance` is a strong join only when both sides bind
the same `request_envelope_digest` under the same declared canonicalization. If
the tunnel artifact and the referenced execution or policy artifact do not
share canonicalization, the relationship should be treated as diagnostic
correlation, not a proven instance binding. Raw JSON-RPC `id` is not enough for
this join because it may be a low-entropy per-connection counter.

#### `non_claims`

This section is required. It prevents the artifact from reading like a security
proof.

Recommended v0 values:

- `agent_identity_not_verified_by_tunnel_observation`
- `authorization_not_proven_by_tunnel_observation`
- `policy_outcome_not_inferred_from_transport`
- `tool_result_truth_not_proven`
- `application_outcome_not_proven`
- `upstream_server_trust_not_proven`
- `token_freshness_not_proven`
- `observed_facts_trust_depends_on_observation_point_integrity`
- `route_facts_may_be_asserted_not_mediation_proven`

For v0, the artifact is unsigned. Its trust level is therefore the trust level
of the emitter and observation point. A future in-toto, Sigstore, or comparable
attestation envelope can lift the artifact from self-reported observation to
attested observation, but P57 should not make that a requirement for the first
sample.

## 9. Representative v0 artifact

```json
{
  "schema": "assay.mcp.tunnel_observed.v0",
  "artifact_id": "mcp-tunnel-observed-001",
  "observed_at": "2026-06-03T16:00:00Z",
  "provider_context": {
    "provider": "example",
    "surface": "mcp_tunnel",
    "component": "tunnel-client",
    "component_version": "0.0.0"
  },
  "tunnel": {
    "tunnel_ref": "tunnel_redacted",
    "tunnel_ref_kind": "provider_id",
    "direction": "outbound_client_poll",
    "transport": "https_long_poll"
  },
  "request_instance": {
    "request_id": "req-001",
    "request_envelope_digest": "sha256:1111111111111111111111111111111111111111111111111111111111111111",
    "request_envelope_canonicalization": "jcs:mcp_request_envelope.v1",
    "nonce": "n-001"
  },
  "route": {
    "channel": "main",
    "method": "tools/call",
    "path": "/mcp"
  },
  "upstream": {
    "target_ref": "local-stdio",
    "target_kind": "stdio",
    "target_digest": "sha256:2222222222222222222222222222222222222222222222222222222222222222"
  },
  "mcp": {
    "method": "tools/call",
    "tool_name": "deploy_service"
  },
  "auth_context": {
    "authorization_header_visible": true,
    "authorization_header_stored": false,
    "authorization_header_digest": "sha256:3333333333333333333333333333333333333333333333333333333333333333",
    "mcp_oauth_metadata_visible": true,
    "client_mtls_configured": false
  },
  "visibility": {
    "request_payload_mode": "digest_only",
    "response_payload_mode": "not_observed",
    "tool_result_visible": false,
    "policy_decision_visible": false,
    "raw_payload_retained": false
  },
  "evidence_refs": [
    {
      "kind": "mcp.execution_record",
      "digest": "sha256:4444444444444444444444444444444444444444444444444444444444444444",
      "relationship": "same_request_instance",
      "join_strength": "strong"
    }
  ],
  "non_claims": [
    "agent_identity_not_verified_by_tunnel_observation",
    "authorization_not_proven_by_tunnel_observation",
    "policy_outcome_not_inferred_from_transport",
    "tool_result_truth_not_proven",
    "application_outcome_not_proven",
    "upstream_server_trust_not_proven",
    "token_freshness_not_proven",
    "observed_facts_trust_depends_on_observation_point_integrity",
    "route_facts_may_be_asserted_not_mediation_proven"
  ]
}
```

## 10. Negative fixture ideas

If P57 graduates into implementation, the first negative fixtures should be:

1. **Substituted tunnel reference**
   - same request digest, different `tunnel_ref`;
   - expected result: join to a tunnel-specific assertion fails.
2. **Substituted upstream target**
   - same request digest, different `upstream.target_digest`;
   - expected result: same-request claim cannot imply same upstream boundary.
3. **Method/tool mismatch**
   - route says `tools/call`, execution record resolves to a different
     method/tool;
   - expected result: pairing fails or is reported as ambiguous.
4. **Raw auth material retained**
   - `authorization_header_stored=true` with raw token payload;
   - expected result: lint failure.
5. **Transport fact overclaim**
   - artifact omits non-claims and asserts `authorized=true`;
   - expected result: schema or lint failure.
6. **Missing-vs-unknown confusion**
   - no `route.channel` but `channel=unknown` is implied downstream;
   - expected result: missing evidence and unknown observed value stay
     distinct.
7. **Forged observation facts**
   - artifact reports `direction=outbound_cloudflared` and a tunnel route, but
     the observation point was bypassable or upstream route headers were
     spoofable;
   - expected result: consumer keeps the route as asserted/observed-by-emitter
     context, not proof that mediation happened.

## 11. Relation to SEP-2828 execution records

SEP-2828-style records answer a different question:

> What did the server say it decided and what outcome did it bind to that
> decision?

P57 answers:

> What did the tunnel observation point see about how this request reached the
> private MCP server?

The two can compose through request instance binding:

```text
tunnel observation
  request_envelope_digest + route/upstream facts
      |
      v
server execution record
  instance binding + decision/outcome pairing
```

P57 should not import SEP-2828 as a dependency. It should only leave a clean
join point when execution records exist.

Join strength matters. A tunnel artifact and a SEP-2828-style execution record
form a strong instance join only when both bind the same canonical request
envelope digest. If the relationship is based on request id, timestamp, route
label, or provider request id alone, Assay should render it as diagnostic
correlation.

## 12. Relation to Camunda RequestSource

Camunda's `RequestSource` direction is a useful adjacent model: source/channel
and tool name are observed context, not proof of agent identity, authorization,
or policy outcome.

P57 should preserve the same distinction:

- `route.channel` and `mcp.tool_name` are observed facts;
- absence of source is different from observed `unknown`;
- source-specific subtypes are better than forcing every channel to carry MCP
  fields;
- UI wording should render "observed via MCP tunnel" rather than "agent did
  this."

## 13. Relation to CrewAI GovernanceDecision

CrewAI's emerging `GovernanceDecision` boundary is policy-decision evidence,
not tunnel evidence.

P57 should keep the join shape simple:

- tunnel artifact can reference a governance decision artifact;
- governance decision can reference request digest or decision id;
- post-tool `GovernanceOutcome` should remain separate;
- vendor evidence belongs under extensions in the governance artifact, not in
  the tunnel artifact.

This keeps Assay from collapsing transport facts into policy facts.

## 14. Future declared-vs-observed hook

P57 is the observed side of a possible future declared-vs-observed comparison.
If MCP standardizes a public server metadata surface such as Server Cards or a
`.well-known` capability document, P57 artifacts could later be diffed against
that declared capability surface.

That future lane should stay separate:

- declared server metadata says what a server claims or advertises;
- P57 says what a tunnel observation point saw for a bounded request;
- Assay can compare declared capabilities to observed behavior without treating
  declarations as truth;
- missing, unknown, declared, observed, and contradicted capability facts should
  remain distinct claim classes.

Do not cite private, draft, or future RC details as public fact until the
declared metadata surface is published and sourceable.

## 15. First implementation sketch

If this lane graduates, the smallest Assay implementation should be a sample,
not a full adapter.

Suggested files:

- `examples/mcp-tunnel-observed-evidence/README.md`
- `examples/mcp-tunnel-observed-evidence/map_to_assay.py`
- `examples/mcp-tunnel-observed-evidence/fixtures/valid.tunnel.json`
- `examples/mcp-tunnel-observed-evidence/fixtures/substituted-upstream.tunnel.json`
- `examples/mcp-tunnel-observed-evidence/fixtures/raw-auth-leak.tunnel.json`
- `examples/mcp-tunnel-observed-evidence/fixtures/valid.assay.ndjson`

The example should:

- freeze one provider-neutral tunnel observation artifact;
- map it to canonical evidence without assuming provider trust;
- emit explicit lossiness and non-claim metadata;
- avoid raw request/response payloads;
- avoid any public claim that Assay verifies OpenAI, Anthropic, Tapinto, or any
  other provider's tunnel security.

## 16. Outreach posture

Do not open a public thread just to introduce this plan.

Good triggers:

- an MCP tunnel project asks about audit, inspector events, support bundles, or
  evidence exports;
- a provider thread starts treating tunnel observations as authorization or
  agent identity;
- a maintainer asks for fixture/verifier input around request binding;
- a concrete tunnel artifact or inspector event export appears.

Bad triggers:

- generic launch announcements;
- tunnel marketing pages;
- no public artifact shape to discuss;
- desire to mention Assay without a concrete boundary question.

Recommended public posture when triggered:

> The useful split is tunnel-observed facts vs policy/server/tool facts. A
> tunnel artifact can make route, upstream, and request-envelope observations
> reviewable, but it should not by itself claim agent identity, authorization,
> policy outcome, or tool truth.

## 17. Acceptance criteria for promoting P57

Promote from planning to implementation only when all are true:

1. At least one tunnel or MCP-aware inspector exposes a small serialized event
   or support artifact that can be frozen locally.
2. The artifact can be mapped without raw token, raw params, or raw result
   retention.
3. The sample can include at least two negative fixtures.
4. The output keeps tunnel facts, policy decisions, server execution records,
   and tool outcomes separate.
5. The resulting docs can say what Assay does not prove in one sentence.

## 18. Stop lines

Do not proceed if the work requires:

- access to private provider APIs;
- asserting tunnel security;
- validating provider certificates or tunnel tokens;
- importing raw MCP payloads by default;
- making Assay an OAuth verifier;
- making Assay an issuer-trust verifier;
- adding provider-specific code before a provider-neutral fixture exists.
