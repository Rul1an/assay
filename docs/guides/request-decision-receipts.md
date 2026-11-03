# Experimental request–decision receipts

`assay mcp wrap --decision-log decisions.jsonl --event-source assay://local/probe
--request-receipts-out requests.jsonl -- <server> <args>` optionally records the
original valid `tools/call` request line alongside the actual emitted decision.
Use a fresh decision log for each capture. The sidecar is disabled by default,
checked before spawn, created exclusively at the first client frame that is not a
`server/discover` request (mode 0600 on Unix), and must not alias another output. It contains raw arguments and caller metadata: keep it private. Ask and
answer lines can hold user data (prompts, secrets, approvals) and are stored as
the original bytes. `redact_args` does not cover them. Operators who cannot
retain that content must not pass `--request-receipts-out`. Initialize/auth
frames are not recorded. Readers must filter by `schema`: tools/call receipts
are `assay.request_decision_receipt.v0` and stay one per decision.

Each tools/call NDJSON record has experimental schema `assay.request_decision_receipt.v0`:

- `request_line`, `request_bytes`, `request_sha256`: the UTF-8 line exactly as
  received, including its terminator, its byte length, and lowercase SHA-256.
- `decision_event_id`, `decision_sha256`: the existing event ID and SHA-256 of
  that event's compact JSON plus LF, as written by the decision-log emitter.
- `rpc_id_present`, `rpc_id`: presence and original JSON type/value. Missing and
  explicit null stay distinct here even where the decision projection collapses them.
- `scope`, `sequence`: a fresh local capture UUID and zero-based occurrence index
  shared by tools/call receipts and ask/answer observations. After filtering by
  schema, receipt sequences are a strictly increasing subsequence, not a dense
  `0..n-1`, because observation lines occupy the same counter. These are
  coordinates, not agent, vendor-session or application identifiers.
- `metadata_pointer`: `/params/_meta` in the original request. For example, a
  caller's `claudecode/toolUseId` is at `/params/_meta/claudecode~1toolUseId`.
- `declared_protocol_version_state`, `declared_protocol_version`: what this request
  states at `_meta["io.modelcontextprotocol/protocolVersion"]`. The state is `present`
  with the version, `absent`, or `malformed` when the value is not a calendar-date
  version or its container is not an object; the version is null unless `present`.
  This is the request's statement, not a negotiated or verified revision.
- `assertion_basis`: `caller_asserted`. Metadata is supplied by the caller; the
  association is reported by the proxy, not independently authenticated.

Later lines in the same file can be ask or answer observations, experimental
schema `assay.request_observation.v0`. They share the sidecar, its `scope`, its
sequence (so receipts and observations interleave on one counter), and the
1 MiB / 48-record / 4 MiB budget. Each carries `kind` `ask` or
`answer`, the original line, and a `call_link` of `unique`, `ambiguous`, or
`unlinked`. A unique link is only an identifier wrap actually saw: one pending
`tools/call` JSON-RPC id for a legacy server request, or the id already on an
`input_required` result, recorded as `call_id`. Two pending calls are
`ambiguous` with no `call_id`. Overflow of the pending-call set is also
`ambiguous` for the rest of the capture: Unique would guess a recency owner, and
Unlinked would claim no pending call was observed. A `sampling/createMessage`
with no pending call, and an answer whose id matches no recorded ask, are
`unlinked` — not attached to an earlier completed call. `requestState` and
`inputRequests` keys are not extracted or recorded as link identifiers; they
remain in the original bytes. A modern retry is its own tools/call receipt, and
readers correlate it from those bytes. These lines are observations of frames
wrap already forwards, not a wrap decision and not proof a human answered.
Accept and decline are distinguishable because the original answer bytes differ.

The observation window is this proxy invocation. Retain its launch/stop provenance
and the referenced events' existing timestamps; neither scope nor ordering creates
cross-source clock agreement. Existing `tool_call_id`, JSON-RPC IDs, event IDs,
policy decisions, forwarding bytes and strong-key admission are unchanged. In
particular `req_`/`gen_` IDs remain ineligible as strong keys. The whole mixed sidecar can be retained as context with the v6.22.0
`prepare-incident-package --request-receipts` opt-in; this does not admit receipts as
action or join evidence. A denied call can have a receipt without reaching the server.

Capture checks a 1 MiB request limit before making an additional copy, stops at
48 records, and bounds total serialized output to 4 MiB, counting tools/call
receipts and ask/answer observations together. Decision serialization
for hashing also has a 1 MiB ceiling. This bounds additional receipt storage,
not the existing proxy `read_line` allocation. A limit or write failure disables
further capture, retains any prefix, and writes a fixed `capture incomplete`
diagnostic. An oversize or over-budget ask is that failure, never a silent drop
that leaves a complete-looking sidecar. The original decision emitter runs first; policy and forwarding
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
consumer assertions are test controls, not a new production verifier. Ask/answer
retention is covered by `cargo test --locked -p assay-cli --test mcp_wrap_server_asks --no-default-features`.
