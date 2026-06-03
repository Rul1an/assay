# MCP Tunnel Observed-Facts Evidence Sample

This example turns one tiny provider-neutral MCP tunnel observation artifact
into bounded, reviewable external evidence for Assay.

It is intentionally small:

- start from one serialized `assay.mcp.tunnel_observed.v0` sample artifact
- keep tunnel route and upstream facts separate from the request binding key
- map the good artifact into an Assay-shaped placeholder envelope
- reject raw authorization material and strong joins with mismatched request
  canonicalization
- keep optional inspector event references to bounded `kind` / `digest` /
  optional opaque `ref` objects
- keep tunnel presence, policy decisions, agent identity, and tool outcome
  truth out of Assay truth

## What is in here

- `map_to_assay.py`: maps one MCP tunnel observed-facts artifact into a
  placeholder Assay envelope
- `fixtures/valid.tunnel.json`: one bounded successful tunnel observation
- `fixtures/substituted-upstream.tunnel.json`: same request binding with a
  different observed upstream, useful for reviewing route/upstream separation
- `fixtures/raw-auth-leak.tunnel.json`: malformed import case that stores raw
  authorization material
- `fixtures/valid.assay.ndjson`: mapped placeholder output with a fixed import
  time
- `test_map_to_assay.py`: stdlib unit tests for the mapper boundary

## Why this seam

MCP tunnels can observe useful request-routing facts: tunnel instance, channel,
method, upstream target boundary, and a bounded request envelope digest. Those
facts are useful for review, but they are not proof of:

- agent identity
- authorization correctness
- policy outcome
- tool result truth
- application outcome
- upstream server trust
- unbypassable mediation

The request binding is deliberately small. It includes only
`request_envelope_digest` plus `request_envelope_canonicalization`. Route,
channel, and upstream labels stay under `route` and `upstream`, because a
server-side execution or policy record may never observe the tunnel route. A
strong instance join is only possible when both sides bind the same request
envelope digest under the same canonicalization. Request id, timestamp,
provider request id, or route label alone are diagnostic correlation.

## Map the checked-in valid artifact

```bash
python3 examples/mcp-tunnel-observed-evidence/map_to_assay.py \
  examples/mcp-tunnel-observed-evidence/fixtures/valid.tunnel.json \
  --output examples/mcp-tunnel-observed-evidence/fixtures/valid.assay.ndjson \
  --import-time 2026-06-03T18:00:00Z \
  --overwrite
```

## Verify the observed-facts fixture

```bash
assay evidence verify-mcp-tunnel-observed \
  --artifact examples/mcp-tunnel-observed-evidence/fixtures/valid.tunnel.json \
  --format json
```

This checker is the reference fixture path for the sample. It validates the
bounded `assay.mcp.tunnel_observed.v0` shape, keeps raw payload/auth material
out, and classifies `evidence_refs` as either strong `same_request_instance`
joins or diagnostic correlation. Strong joins require a shared
`request_envelope_digest` and `request_envelope_canonicalization`; route,
upstream, request id, timestamp, or provider request id alone stay diagnostic.

## Map the substituted-upstream artifact

```bash
python3 examples/mcp-tunnel-observed-evidence/map_to_assay.py \
  examples/mcp-tunnel-observed-evidence/fixtures/substituted-upstream.tunnel.json \
  --output /tmp/substituted-upstream.assay.ndjson \
  --import-time 2026-06-03T18:05:00Z \
  --overwrite
```

This command should succeed. The substituted upstream changes observed route
context, not the request binding key. A reviewer can compare the mapped
`upstream` section without pretending it changes the server-side request
envelope digest.

## Check the raw-auth-leak case

```bash
python3 examples/mcp-tunnel-observed-evidence/map_to_assay.py \
  examples/mcp-tunnel-observed-evidence/fixtures/raw-auth-leak.tunnel.json \
  --output /tmp/raw-auth-leak.assay.ndjson \
  --import-time 2026-06-03T18:10:00Z \
  --overwrite
```

This command is expected to fail because the artifact stores raw authorization
material. That is a boundary rejection, not only parser hygiene:

- this lane may carry redacted or digest-only auth context
- this lane does not preserve bearer tokens
- observing an auth header does not prove authorization succeeded

## Important boundary

This mapper writes sample-only placeholder envelopes.

It does not:

- register a new Assay Evidence Contract event type
- claim that any tunnel provider emits this exact artifact
- verify tunnel integrity or unbypassable mediation
- verify OAuth, bearer-token, or mTLS correctness
- verify server execution records, policy decisions, or tool outcomes
- treat route, channel, upstream, request id, or timestamp as a strong join key
- accept raw inspector event payloads under `inspector_event_refs`

The checked-in sample is unsigned. Its trust level is therefore the trust level
of the emitter and observation point. A future attestation envelope could
strengthen that, but this first sample keeps the consumer boundary explicit.

## Run tests

```bash
python3 -m unittest examples/mcp-tunnel-observed-evidence/test_map_to_assay.py
cargo test -p assay-cli --test evidence_test mcp_tunnel_observed
```
