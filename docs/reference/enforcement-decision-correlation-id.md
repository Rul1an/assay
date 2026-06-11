# Enforcement-decision correlation id (forward-looking spec, no code)

**STATUS: review-spec. Not implemented.** This is the producer-side design for an optional per-call
correlation id on the `assay.enforcement_decision.v0` record. It is the clean join-key that lets a
harness or consumer match a decision record to the call that produced it and to what an upstream
observed, without depending on call order. It does **not** change anything shipped today; both current
consumers — an enforcement-proof harness and the Plimsoll release-review consumer — run on order+content
correlation and stay valid unchanged.

## Why

`assay.enforcement_decision.v0` is deliberately **deterministic** and carries **no request-id and no
timestamp** (determinism for reproducibility; no timestamp so the record makes no transport/timing
claim). A record is therefore correlated to the call that produced it, and to an upstream's observed
arrival, only by **order + content** — `tool.name` plus the projected `action.target` /
`target_digest`, in append order. That is sufficient for a deterministic single-caller sequence (an
enforcement-proof harness scores exactly such a sequence), but it is fragile under concurrency, retries, or repeated
identical calls, and it is the one stated limitation of an enforcement-proof harness scoring such a
sequence. A correlation id removes that dependence.

## What it is

An optional, **opaque, caller-supplied** per-call token that the proxy **echoes** into the decision
record and forwards onward, so three things join on the same key: the caller's request, the decision
record, and a cooperating/controlled upstream's observation.

- **Caller-supplied, proxy-echoed — never proxy-minted.** The caller (agent / driver / client) puts
  the token on the request; the proxy copies it verbatim into the record and onto the onward request.
  The proxy generates **no** entropy and **no** timestamp. This is the load-bearing rule: the record
  must stay a deterministic function of its inputs, and a proxy-minted random/time id would break that
  invariant. The id is an *input*, so determinism is preserved.
- **Optional.** Absent on the request → absent (or `null`) on the record → consumers fall back to
  order+content (today's behaviour). Presence is never required and its absence is never a finding.
- **Opaque.** A join-key only: no semantics, no PII, no request content, no args, no ordering or
  sequence guarantee. Treat it as a bag of bytes for equality.

## What it is NOT

- **Not a timestamp** and not a sequence number — it carries no time or order meaning.
- **Not a transport or delivery claim.** Its presence says nothing about whether the call reached or
  was performed by the upstream. An `allow` with a correlation id is still only the decision to
  forward; delivery proof remains the harness's job, and side effects stay asserted (E9 ladder).
- **Not an identity field.** Caller identity stays `caller.id`; the correlation id does not identify a
  caller and must not be reused as one.

## Transport (echo path)

- **Caller → proxy:** the token rides in MCP request `_meta` (the protocol's extensibility slot) under
  the one reserved key (below).
- **Proxy → record:** copied into the decision record **only after it passes the safe-shape check
  below**; a token that fails validation is dropped (the record carries no `correlation_id`, or an
  explicit `correlation_id_invalid` diagnostic — see below), never the raw bytes.
- **Proxy → upstream:** the same validated token is forwarded in the onward request `_meta`. A real
  upstream that ignores `_meta` is unaffected; in that case driver↔record correlation alone already
  removes the harness's order-dependence (a controlled upstream that *does* read it gives the full
  three-way join).

## Field shape

A single optional field on the existing per-call record:

```json
{
  "schema": "assay.enforcement_decision.v0",
  "caller": { "id": "ci-agent" },
  "correlation_id": "9f2c…opaque…",      // optional; absent or null when the caller supplied none
  "...": "all existing v0 fields unchanged"
}
```

## Safe shape (the id is evidence — bound it before it lands)

The token is caller-controlled and ends up in a persisted evidence artifact, so the proxy must treat it
as hostile input and pin a bounded shape — otherwise the correlation id becomes a channel for smuggling
content (PII, secrets, oversized blobs, control characters) into the evidence stream. The producer
validates before it copies, and **never** persists raw unvalidated bytes:

- **One reserved `_meta` key**, fixed and documented (e.g. `assay.correlation_id`); any other key is
  ignored. No free-form metadata is hoovered into the record.
- **String only.** Non-string (object, array, number, bool) → rejected.
- **Bounded length.** A small fixed cap (e.g. ≤ 128 chars); over-length → rejected, never truncated
  (truncation would forge a different id).
- **Constrained charset.** A safe, opaque alphabet only — recommend a UUID, or `base64url` / hex
  (`[A-Za-z0-9_-]`). No whitespace, no control characters, no terminal escapes (the same hostile-input
  discipline already applied to rendered evidence strings).
- **Opaque by contract.** It is a join-key, not a place for content. The spec states plainly that
  callers must not put PII or secrets in it; the charset/length bound makes accidental smuggling hard,
  and the value-free rule keeps it from being mistaken for data.
- **Invalid → omit or diagnose, never trust.** A token that fails any check is dropped: the record
  omits `correlation_id` and falls back to order+content, **or** carries an explicit boolean
  `correlation_id_invalid: true` diagnostic (no raw value) so the gap is visible rather than silent.
  Pick one and pin it in the implementation; the default is omit-with-fallback.

This keeps the id a safe equality key and never an evidence leak. Determinism is unaffected — a fixed
valid input yields a fixed record; a rejected input yields the deterministic fallback.

## Versioning decision (the one real choice — recommend additive within v0)

This is where the design must be pinned, because it decides whether any consumer breaks.

- **RECOMMENDED — additive optional field *within* `assay.enforcement_decision.v0`, schema string
  unchanged.** This matches the house precedent for additive metadata: `field_digests` was added to
  `assay.mcp_manifest_observed.v0` as *"a purely additive, append-only extension within v0 — there is
  no schema bump"* (see [mcp-manifest-drift.md](mcp-manifest-drift.md)). Current consumers ignore the
  unknown field; new consumers use it. Crucially it does **not** trip the Plimsoll consumer's
  `unsupported_schema` path — which gates the release when decisions are expected — so no consumer
  breaks and no transition is needed.
- **ALTERNATIVE — an `assay.enforcement_decision.v1` schema-string bump.** Conceptually tidy, but
  **breaking**: every existing v0 consumer (Plimsoll today, an enforcement-proof harness) classifies a `v1` record
  as `unsupported_schema` and gates until updated, and any content-addressing keyed on the schema
  string changes. Per the append-only enum discipline, reserve an explicit `v1` bump for a genuinely
  breaking or semantic change — adding an ignorable optional join-key is neither.
- **Recommendation:** ship the correlation id as the additive optional field within v0. Treat
  "`enforcement_decision.v1`" as the *name reserved for a future breaking change*, not the mechanism
  for this one. (Flagged because this was originally framed as a "v1 evolution"; the capability is the
  v1 idea, the mechanism that avoids breaking consumers is additive-within-v0.)

## Consumer / harness use (when shipped)

- **An enforcement-proof harness:** when `correlation_id` is present on both the decision record and
  the controlled upstream's received-call log, join on it (exact, concurrency-safe); otherwise fall
  back to order+content. The harness stays on v0; only its correlation step gains an exact path, and
  its order+content limitation is what this design removes.
- **Plimsoll (release review):** the id is descriptive only — it may enable a per-call / per-id view or
  de-dup, but gate semantics are unchanged. An allow with a correlation id is still not a delivery
  claim.

## Out of scope

Delivery proof (the harness's job, from a controlled upstream — arrival ≠ side effect); multi-caller
identity (`caller.id` stays the identity); any new gate or required field; any change to the existing
v0 fields or their canonicalization.

## Implementation steps (when greenlit — a separate PR)

1. Reserve the one `_meta` key and the optional record field name; document the absent/null fallback
   and the chosen invalid-handling (omit-with-fallback vs `correlation_id_invalid` diagnostic).
2. Producer: read the caller `_meta` token, run the **safe-shape check** (string, ≤ cap, safe charset,
   no control chars), and only then copy the validated token into the record and onto the onward
   request; emit nothing (or the diagnostic) when absent or invalid — never the raw bytes.
3. Producer guard tests: the record stays **deterministic** given a fixed valid id; the field is absent
   when the caller supplied none; and a too-long / wrong-charset / non-string / control-char token is
   rejected (record omits it or sets the diagnostic, never persists the raw value). No proxy-minted
   value, no timestamp.
4. The enforcement-proof harness: add the join-on-id path with order+content fallback.
5. Plimsoll: optional per-id view; gate semantics unchanged.
