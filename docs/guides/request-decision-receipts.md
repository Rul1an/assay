# Experimental request–decision receipts

`assay mcp wrap --decision-log decisions.jsonl --event-source assay://local/probe
--request-receipts-out requests.jsonl -- <server> <args>` optionally records the
original valid `tools/call` request line alongside the actual emitted decision.
Use a fresh decision log for each capture. The sidecar is disabled by default,
checked before spawn, created exclusively at the first client frame that is not a
`server/discover` request (mode 0600 on Unix), and must not alias another output. It contains raw arguments and caller metadata: keep it private. No
initialize/auth frames or non-tool messages are recorded.

Each NDJSON record has experimental schema `assay.request_decision_receipt.v0`:

- `request_line`, `request_bytes`, `request_sha256`: the UTF-8 line exactly as
  received, including its terminator, its byte length, and lowercase SHA-256.
- `decision_event_id`, `decision_sha256`: the existing event ID and SHA-256 of
  that event's compact JSON plus LF, as written by the decision-log emitter.
- `rpc_id_present`, `rpc_id`: presence and original JSON type/value. Missing and
  explicit null stay distinct here even where the decision projection collapses them.
- `scope`, `sequence`: a fresh local capture UUID and zero-based occurrence index.
  These are coordinates, not agent, vendor-session or application identifiers.
- `metadata_pointer`: `/params/_meta` in the original request. For example, a
  caller's `claudecode/toolUseId` is at `/params/_meta/claudecode~1toolUseId`.
- `declared_protocol_version_state`, `declared_protocol_version`: what this request
  states at `_meta["io.modelcontextprotocol/protocolVersion"]`. The state is `present`
  with the version, `absent`, or `malformed` when the value is not a calendar-date
  version or its container is not an object; the version is null unless `present`.
  This is the request's statement, not a negotiated or verified revision.
- `assertion_basis`: `caller_asserted`. Metadata is supplied by the caller; the
  association is reported by the proxy, not independently authenticated.

The observation window is this proxy invocation. Retain its launch/stop provenance
and the referenced events' existing timestamps; neither scope nor ordering creates
cross-source clock agreement. Existing `tool_call_id`, JSON-RPC IDs, event IDs,
policy decisions, forwarding bytes and strong-key admission are unchanged. In
particular `req_`/`gen_` IDs remain ineligible as strong keys. No receipt is admitted
by incident-v1. A denied call can have a receipt without reaching the server.

Capture checks a 1 MiB request limit before making an additional copy, stops at
48 records, and bounds total serialized output to 4 MiB. Decision serialization
for hashing also has a 1 MiB ceiling. This bounds additional receipt storage,
not the existing proxy `read_line` allocation. A limit or write failure disables
further receipts, retains any prefix, and writes a fixed `capture incomplete`
diagnostic. The original decision emitter runs first; policy and forwarding
continue. A refused path instead aborts before spawning the server, and a path taken after
spawn ends the run before that frame is forwarded. A run that saw only `server/discover`
creates no file; see [the wrap reference](../reference/cli/mcp-server.md).

A receipt is not a completion or durability marker. The decision emitter is
best-effort and may fail independently. Before using a capture, resolve each
reference against retained original decision-log bytes; compare exact event-ID
populations and multiplicities, both digests and lengths, one expected local
scope, contiguous occurrence indices, request ID types/presence and tool name.
Reject malformed/truncated lines, duplicate, omitted, surplus or unresolved
references, and captures with incomplete diagnostics or missing lifecycle evidence.
Do not pick first/latest on ID reuse. Preserve failed artifacts. Hash agreement
only detects changes relative to the retained reference; it cannot authenticate a
jointly rewritten pair or establish that unobserved calls did not occur.

The supported conclusion is that these retained proxy bytes associate this exact
received frame with this retained decision event. It does not prove forwarding,
delivery, execution, vendor identity, vendor-source admission, hook linkage or
per-call kernel causality. Full A/B/C remains on hold. A controlled host capture
and a separately reviewed cross-source contract are still required.

The synthetic regression runs through the actual CLI and stdio proxy:

```sh
cargo test --locked -p assay-cli --test mcp_wrap_request_receipts --no-default-features
```

It needs Python 3 but no credentials, model invocation or external service. Its
consumer assertions are test controls, not a new production verifier.
