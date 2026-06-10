# MCP upstream proxy mode — manifest-observation v0 (P61)

Status: **shipped in assay v3.23.0** — manifest-observation v0 (P61a design, P61b forwarding skeleton,
P61c live `tools/list` observation + `assay.mcp_manifest_observed.v0` emission, P61d denied-method
hardening). The enforcing `tools/call` proxy (P61e) is a separate, not-yet-specified arc — a heavier
security boundary (caller authorization, upstream credential use, a policy decision before forwarding,
confused-deputy prevention, `proxy_denied` semantics, side-effect-evidence interaction) that needs its
own review-spec before any code. This doc remains the design of record for the shipped manifest-
observation mode. Related: [mcp-manifest-drift.md](mcp-manifest-drift.md) (the artifact this mode
feeds) and the [privileged-action evidence](privileged-action-evidence.md) set.

The P61 v0 proxy is an opt-in manifest-observation proxy. It observes upstream `tools/list` traffic
and emits manifest evidence with honest completeness. It does not execute tools through the proxy.

## The decision (Q1): terminating server vs upstream proxy

`assay-mcp-server` today **terminates** the JSON-RPC protocol and serves its own built-in tools (the
P60b2 topology finding). This spec adds upstream proxying as an **explicit, opt-in run mode** — never
implicit, never the default.

- the terminating policy-server stays the default and is unchanged;
- proxy mode is a deliberate operator choice (an explicit run mode plus a configured upstream), because
  its security surface — credentials, confused-deputy, forwarding failure modes — must be chosen, not
  inherited by pointing at an upstream;
- **a transparent, credential-holding proxy that silently relays everything is explicitly rejected.**
  That shape is the confused deputy this spec exists to avoid.

## v0 scope is LOCKED: manifest-observation only

v0 is a **manifest-observation proxy**. It exists to observe a live upstream tool manifest, nothing
more:

- forwards the `initialize` / `initialized` handshake to the upstream;
- forwards `tools/list` and observes the response read-only (the full v0 allowlist is below);
- tracks the `tools/list` pagination chain and emits `assay.mcp_manifest_observed.v0` (the P60b
  producer), with honest completeness;
- **does not forward privileged `tools/call`.** A `tools/call` (and any method not on the allowlist)
  returns a distinct proxy-space `proxy_unsupported` response and is never relayed to the upstream.

**Scope guard (state this whenever P61a is cited):** P60 remains artifact/file-based until P61 exists.
P61 v0 only enables live upstream manifest observation. P61 v0 does **not** enable tool-call execution
through the proxy.

**There is no observe-only forwarding of privileged `tools/call` in v0.** A proxy that holds upstream
credentials and relays privileged calls without a blocking decision is the confused-deputy trap. So
v0 simply does not forward them. If `tools/call` forwarding is ever added, it must be **enforcing and
fail-closed** (a privileged call is never forwarded unless a policy decision point allows it) — that is
a separate later arc (P61e), not part of this mode.

## Claim and non-claims

**Claim (v0):** Assay can observe the live upstream `tools/list` surface in an explicit stdio proxy
mode and emit manifest evidence with honest completeness.

**Non-claims:**
- **does not support privileged `tools/call` forwarding in v0** (it is explicitly unsupported, not
  silently relayed);
- observing a `tools/list` is observation, not a guarantee about the upstream's full tool set when the
  chain was `partial`/`unknown`;
- proxy mode is not a sandbox and not a guarantee against a malicious upstream; it observes a declared
  surface, it does not contain the upstream;
- no maliciousness classification anywhere; manifest drift stays canonical-digest evidence;
- the proxy is not an authorization server; it does not mint, broaden, or validate caller authority.

## Topology (v0)

```
MCP client  ──stdio──►  assay-mcp-server (proxy mode, v0)  ──stdio──►  upstream MCP server
                              │  read-only observer (tools/list -> manifest)
                              │  allowlist forwarder (handshake + list/read)
                              └─ evidence emitter (assay.mcp_manifest_observed.v0 + observation health)
```

- **Transport (decision A):** stdio upstream only in v0. HTTP upstream is a follow-up — it adds TLS,
  upstream identity verification, and network auth surface that v0 should not carry.
- **Multiplicity:** one upstream per proxy process in v0. Multi-upstream multiplexing is a follow-up
  (it changes server-identity labeling and artifact keying), but the identity fields are shaped now so
  adding it later is not a schema break.
- **serverInfo (decision D):** the upstream's `serverInfo` is passed through to the client (the client
  is talking to the real upstream); the proxy records that Assay was in path in evidence rather than
  impersonating either side.

## Forwarding semantics (v0)

- **Method allowlist (exhaustive for v0).** v0 forwards only these methods: `initialize`,
  `notifications/initialized`, `ping`, `tools/list`, and the `notifications/tools/list_changed`
  notification. Nothing else is forwarded — first and foremost `tools/call`. There are deliberately no
  other read/list methods in v0: the goal is live manifest observation, not general read-only MCP
  forwarding, and "is this list/read method safe — could it return data/PII/secrets?" is a question v0
  refuses to open. A broader allowlist is a future decision, not v0.
- **id correlation.** 1:1 request-id passthrough; the proxy originates **no** requests of its own in
  v0 (no relisting on the client's behalf), so there is no id remapping.
- **notifications.** forwarded in the correct direction; `notifications/tools/list_changed` is observed
  as a run fact (see manifest observation) and passed through.
- **handshake.** `initialize`/`initialized` are forwarded so the client negotiates capabilities with
  the real upstream; the proxy observes the negotiated capabilities, never fakes them.
- **no mutation of forwarded responses.** the proxy never mutates forwarded upstream responses. The
  only client-visible responses originated by the proxy are proxy-space failure/unsupported responses
  (and, in the future enforcing arc, `proxy_denied`).
- **unsupported methods.** a non-allowlisted method (including `tools/call`) gets a `proxy_unsupported`
  error, never a silent relay and never a fabricated success.

### Non-allowlisted method matrix (the denied boundary, P61d)

Manifest-observation proxy mode is **not** a general MCP proxy. Every method outside the exhaustive
allowlist above is denied and the upstream receives none of it:

| Client message | v0 behavior |
|----------------|-------------|
| `tools/call` | `proxy_unsupported` (request), never forwarded |
| `resources/read`, `resources/list` | `proxy_unsupported`, never forwarded |
| `prompts/get`, `prompts/list` | `proxy_unsupported`, never forwarded |
| `sampling/createMessage`, `completion/complete` | `proxy_unsupported`, never forwarded |
| any unknown/custom method (request) | `proxy_unsupported`, never forwarded |
| any non-allowlisted client **notification** (no id) | dropped (no id to answer), never forwarded |

A denied request always carries `data.origin: "assay-proxy"` and `data.reason: "method_not_allowlisted"`
so it is distinguishable from an upstream error. This is hardening of the negative boundary, not a new
capability: there is no policy decision point, no `proxy_denied`, no credential-scope, and no consumer
logic here.

## `tools/list` observation + pagination (the payoff)

- forward `tools/list`; observe the response read-only.
- **pagination chain:** correlate the client's `tools/list` sequence (each carrying `cursor = prior
  nextCursor`) per `(session, upstream)` into one logical list operation; accumulate tool definitions
  across pages. Completeness:
  - `complete` — observed from a cursor-less first page through a terminal page with no `nextCursor`;
  - `partial` — a chain started but a `nextCursor` was still outstanding at session end, or an error
    interrupted it;
  - `unknown` — a `tools/list`-shaped response was seen but the chain cannot be proven whole (joined
    mid-stream).
  Hard rule: **`complete` requires observing the whole chain start→terminal.** Seeing only the last
  page is `unknown`. **Partial/unknown is never clean.**
- the proxy **does not re-issue** `tools/list` to fill gaps (no proxy-originated requests).
- feed the accumulated tool set into the existing P60b producer → `assay.mcp_manifest_observed.v0`,
  latest-complete-else-best-observed, one artifact per run; duplicate names → `status: ambiguous`.
- `notifications/tools/list_changed`: recorded as run facts (`observed_list_operations`,
  `tools_list_changed_observed`), not per-list snapshots (snapshots/diffing are P60d).

## Server / proxy identity

- v0 records the **real upstream identity** (a configured upstream id, plus the upstream's negotiated
  `serverInfo` where available) on observed evidence, replacing the current constant label.
- the **proxy's own identity** is recorded separately, so evidence distinguishes "Assay was in path"
  from "the upstream said".
- a session/connection id correlates a run's observations to one upstream conversation.

## Credential boundary and confused-deputy threat (must hold from day one)

Even though v0 forwards no privileged calls, these rules are locked now so no later slice can erode
them:

- **no token passthrough by default.** Inbound client auth headers/tokens are **not** forwarded to the
  upstream. (This is already an asserted invariant in the codebase — the no-passthrough security test:
  outbound headers come only from the proxy's own configuration, inbound is never forwarded.)
- **no `Authorization` header forwarding unless explicitly configured** per deployment, with a loud
  config name and a documented threat model — never the default.
- **upstream credentials come only from the proxy's own configuration**, referenced by **alias** in
  evidence, never by value; raw key material is never stored.
- **confused-deputy posture.** A proxy holding an upstream credential must never let an inbound caller
  borrow that credential to reach what the caller was not authorized for. In v0 this is enforced
  trivially — no privileged `tools/call` is forwarded, so caller-induced privileged upstream actions
  cannot occur. Any future forwarding arc (P61e) must add **per-caller authorization at a policy
  decision point before forwarding**, least-privilege on the upstream credential (ties into the shipped
  credential-scope evidence), and must not upgrade or broaden caller authority. No silent mixing of
  Assay's own tools with the upstream's tools (a spoofing/poisoning vector).

## Failure semantics

Default to honest-inconclusive for observation; never fabricate.

- **upstream unreachable / spawn failure:** return a `proxy_failed` error; if a manifest output path is
  configured, emit `status: not_observed`.
- **upstream timeout:** `proxy_failed`; the in-flight manifest chain becomes `partial`/`unknown`.
- **malformed upstream response:** never pass garbage through as truth and never decode-and-trust;
  surface `proxy_failed`.
- **non-allowlisted method (incl. `tools/call`):** `proxy_unsupported`, never forwarded.
- **mid-pagination session end / upstream crash:** flush the best-observed manifest with honest
  completeness; never claim `complete`.
- **distinct error space:** proxy-originated errors are distinguishable from upstream errors —
  `upstream_error` (the upstream said no), `proxy_denied` (reserved for the future enforcing arc),
  `proxy_failed` (the proxy could not complete), `proxy_unsupported` (method not handled in this mode).
  A client can always tell who answered.

## Evidence artifacts

- **observation artifacts (read-only facts):** `assay.mcp_manifest_observed.v0` (the manifest), plus a
  small **proxy observation-health** record (did `initialize` complete, was the list chain whole, were
  responses lost) so downstream never reads silence as "clean."
- atomic write + final flush; an output artifact is never absent when requested (`status:
  not_observed` instead).
- **no enforcement record in v0** — v0 enforces nothing (it forwards no privileged calls). Any future
  enforcing arc emits enforcement evidence in its own carrier, kept separate from observation, so an
  observation artifact never implies an enforcement that did not happen.
- all artifacts carry explicit `non_claims`.

## Hardening / resource limits

Apply the existing config knobs to forwarded traffic too (`timeout_ms`, `max_msg_bytes`,
`max_field_bytes`, `max_tool_calls`, cache): bound upstream response size, cap concurrent in-flight,
time out the upstream, and treat all upstream bytes as hostile input. A malicious or runaway upstream
must not exhaust the proxy.

## Out of scope for v0

Privileged `tools/call` forwarding (no observe-only, no enforcing — out of this mode); HTTP upstream
transport; multi-upstream multiplexing; token minting / OAuth resource-server behavior; per-tool
granular drift (P60d); proxy-originated relisting; sandboxing the upstream; any maliciousness
classification.

## PR sequence (P61) — no privileged `tools/call` forwarding in P61b–d

```
P61a  this proxy-mode spec (design doc, no code)                                            [SHIPPED v3.23.0]
P61b  stdio upstream connection manager + initialize/initialized + tools/list forwarding (allowlist),
      no-token-passthrough invariant + failure semantics + the negative forwarding invariant tested  [SHIPPED v3.23.0]
P61c  tools/list pagination tracker + emit assay.mcp_manifest_observed.v0 + proxy observation-health   [SHIPPED v3.23.0]
P61d  explicit proxy_unsupported behavior for tools/call (and non-allowlisted methods) in
      manifest-observation mode, with tests proving privileged calls are never forwarded               [SHIPPED v3.23.0]
P61e  LATER, separate arc: enforcing tools/call proxy with a fail-closed policy decision point and the
      full confused-deputy mitigations — only if/when specified (review-spec first)                    [NOT STARTED]
```

P61b carries the **negative forwarding invariant test from day one**: a `tools/call` sent in
manifest-observation mode → the upstream receives nothing → the client gets `proxy_unsupported`. The
"never forward privileged calls observe-only" invariant must be testable the moment a forwarding layer
exists; P61d then expands the non-allowlisted-method matrix and its docs.

## Design rules (binding for the whole arc)

- no token passthrough by default;
- no `Authorization` header forwarding unless explicitly configured;
- no raw credentials in evidence (alias only);
- upstream identity recorded; proxy identity recorded separately;
- errors distinguish `upstream_error` vs `proxy_denied` vs `proxy_failed` (vs `proxy_unsupported`);
- no client-visible response mutation except proxy deny/failure/unsupported;
- `tools/list` pagination tracked start→terminal; partial/unknown is never clean;
- privileged `tools/call` is never forwarded observe-only; if forwarded at all, it is enforcing and
  fail-closed.
