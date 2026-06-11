# MCP upstream proxy — enforcing `tools/call` mode (P61e review-spec)

Status: **review-spec, no code.** A new arc and the heaviest risk class in the proxy line. It extends
the shipped, opt-in [manifest-observation proxy](mcp-upstream-proxy-mode.md) (P61a–d, assay v3.23.0)
from observe-only to **enforcing**: it forwards a privileged `tools/call` only after a fail-closed
policy decision. Tracked as [#1624]. Spec only — no code lands until the binding decisions below are
agreed, because forwarding privileged calls with an upstream credential makes the proxy a confused
deputy unless every dimension here is locked first.

The one-line scope: **the manifest-observation proxy answers "did this tool surface change?"; the
enforcing proxy answers "should this specific privileged call be forwarded, right now, for this
caller?" — and defaults to no.**

## 0. Why this is its own arc

P61b–d deliberately never forward `tools/call` (not even observe-only) — a credential-bearing proxy
that relays privileged calls without a blocking decision is the textbook confused deputy. P61e is the
only slice that crosses that line, so it opens, all at once: caller authorization, upstream credential
use on a caller's behalf, a policy decision *before* forwarding, confused-deputy prevention,
`proxy_denied` semantics, the interaction with manifest drift, and the interaction with the
side-effect evidence ladder. Each is specified below; none may be skipped.

## 1. The decision (Q1): enforce, or never forward?

Recommendation: forward a privileged `tools/call` **only enforcing and fail-closed**, never
observe-only. Observe-only forwarding of a privileged call is exactly the trap P61a rejected and stays
rejected. Manifest-observation mode remains the default and unchanged; enforcing mode is a **separate,
explicit opt-in run mode** (its own flag/subcommand plus a policy), never implicit, never the default,
and a deployment that does not opt in keeps P61b–d's behavior (every `tools/call` → `proxy_unsupported`).

## 2. Claim, non-claims, boundary

**Claim (when shipped):** in an explicit enforcing mode, the proxy applies a deterministic,
fail-closed policy decision before forwarding a privileged `tools/call`, denies the call when the
decision is not a clear allow, and records the decision as evidence.

**Boundary sentence:** *the enforcing proxy decides whether to forward a call against a declared
policy; it does not prove the call's side effect occurred, does not verify the upstream's behavior, and
does not classify maliciousness.*

**Non-claims:**
- not an authorization server: it mints no tokens and runs no OAuth flow of its own;
- not a sandbox: an allowed call runs at the upstream with the upstream's own authority;
- a forwarded-after-allow call's side effect stays **asserted**, never proven (the E9 ladder applies);
- `allow` is a policy decision, not a safety guarantee; `deny` is fail-closed caution, not a
  maliciousness verdict;
- no behavior verification, no descriptor semantic analysis, no LLM judge.

## 3. Threat model (SOTA, June 2026)

- **Confused deputy.** A proxy holding a powerful upstream credential is induced by an inbound caller
  to make a privileged upstream call the caller itself was not authorized for. The MCP-specific form is
  acute when a proxy uses a static client identity, allows dynamic client registration, and forwards
  without per-caller consent. Mitigation: §5 (per-caller authorization) + §8 (credential boundary).
- **Post-approval drift (the rug-pull threat).** The tool the caller approved may have mutated since
  approval (OWASP MCP03 post-approval descriptor mutation via `tools/list_changed`; see
  [mcp-manifest-drift.md](mcp-manifest-drift.md) and the E12 lifecycle experiment). A call to a tool
  whose contract digest drifted since approval is invoking something other than what was approved.
  Mitigation: §9 (drift-aware enforcement) — genuinely the novel part of this design.
- **Excessive agency / ambient authority.** The proxy's reach must not silently become the caller's
  reach. Mitigation: §5 + §8.
- **Credential exfiltration / token passthrough.** Inbound caller auth must never be forwarded to the
  upstream, and the upstream credential must never leak into evidence. Mitigation: §8.

## 4. Architecture: a policy decision point before forwarding

```
MCP client ──► assay-mcp-server (enforcing proxy) ──► upstream MCP server
                     │  observe   (P57 tool-decision classification; P60/E12 manifest-drift state)
                     │  DECIDE    (PDP: allow / deny, fail-closed, BEFORE forwarding)  ← P61e
                     │  forward   (only on allow; the upstream runs with its own credential)
                     └─ record    (enforcement decision evidence, separate carrier)
```

The PDP runs only on privileged-classifiable methods (`tools/call`, classified by the P57c
classifiers). Non-privileged allowlisted methods keep the manifest-observation behavior (handshake,
`ping`, `tools/list`); everything else stays `proxy_unsupported`.

## 5. Caller authorization (the confused-deputy core)

The decision to forward a privileged call must consider **who is calling** and **what they are
allowed to do** — not merely "the proxy can reach the upstream."

- **Per-caller consent/authorization.** The proxy keeps a per-caller registry of allowed privileged
  action classes/targets (operator-declared, consistent with the [declared tool surface](declared-tool-surface.md),
  P58). A privileged call with no matching per-caller allowance is denied, never forwarded.
- **Ambient authority is not caller authority.** The proxy's upstream credential never becomes the
  caller's authority: the allowance is keyed to the caller, not to the proxy's reach.
- **No silent surface mixing.** Assay's own tools and the upstream's tools are never merged into one
  list a caller cannot distinguish.

PDP inputs (all deterministic, no LLM): caller identity; the P57c-classified action + projected target;
the declared per-caller allowance (P58); the credential scope the call would use (P59 credential-scope);
and the manifest-drift state of the invoked tool (§9).

## 6. Fail-closed (the core invariant)

If the PDP cannot reach a clear **allow** — policy load error, classifier failure, unknown caller,
inconclusive manifest-drift state, missing per-caller allowance, insufficient credential scope — the
privileged call is **denied** and not forwarded. A proxy that fails open is worse than no proxy.
"Inconclusive" is a deny, never a silent allow.

## 7. `proxy_denied` semantics

`proxy_denied` (the code reserved in P61b, now used) is returned for a policy denial, distinct from
`upstream_error` (the upstream said no), `proxy_failed` (the proxy could not complete), and
`proxy_unsupported` (method not handled in this mode). It carries `data.origin: "assay-proxy"` and a
machine reason: `caller_unauthorized`, `no_declared_allowance`, `manifest_drifted_since_approval`,
`credential_scope_insufficient`, `classification_incomplete`, `policy_unavailable`. A denied call never
reaches the upstream.

## 8. Credential boundary (locked from P61a, extended)

- **No token passthrough by default.** Inbound caller auth is never forwarded to the upstream
  (the standing no-passthrough invariant from P61b).
- **Upstream credential from operator config only**, referenced by alias in evidence, never by value;
  raw key material is never stored.
- **Least privilege on the upstream credential** (ties to P59 credential-scope): the proxy should hold
  only the scope the deployment legitimately needs, and a call requiring more than the upstream
  credential covers is denied.
- **Audience binding (concept-aligned, RFC 8707 / OAuth 2.1).** The proxy must not let a token or
  credential issued for one audience be borrowed to reach another; it does not upgrade or broaden
  caller authority. v0 need not implement token minting, but it must state and enforce that the proxy
  does not broaden authority.

## 9. Drift-aware enforcement (the novel contribution)

Enforcement is tied to the manifest-drift state, connecting P60 (manifest drift), E12 (tool lifecycle),
and P61e (enforcement): a privileged call to a tool whose **contract digest changed since the caller
approved it** is invoking something other than what was approved — the post-approval-mutation payoff. The PDP
therefore checks the invoked tool's current contract digest against the approval-time baseline:

- digest unchanged since approval → the drift gate is satisfied (other gates still apply);
- digest **drifted** since approval → `proxy_denied` (`manifest_drifted_since_approval`), or
  `pending_tool_manifest_review` under a review-mode policy — never a silent allow;
- drift state **inconclusive** (partial/unobserved manifest since approval, per the E12 boundary) →
  fail-closed deny, because "no drift observed" is not "no drift."

This makes the enforcing proxy refuse to forward a privileged call into a tool that moved under it —
which no signing-only (ETDI) or LLM-vetting defense does on the *call* path.

## 10. Side-effect evidence interaction

A forwarded-after-allow call's side effect stays **asserted** unless independently verified — the E9
ladder (`asserted` < `observed_confirmed` < `audit_record_bound`) is unchanged. The enforcement record
notes the proxy *allowed and forwarded* the call; it never claims the upstream performed or persisted
the action. Allowing a call and proving its effect are different, and the proxy only does the first.

## 11. Enforcement decision record (separate carrier)

A new `assay.enforcement_decision.v0` artifact, emitted by the enforcing path and kept **separate** from
the manifest-observation artifact (the standing observation/enforcement separation). It records, per
privileged call: caller identity, the P57c-classified action + projected target (sensitive ids hashed),
the decision (`allow`/`deny`), the machine reason, the fail-closed flag, the drift-gate state, and the
credential alias (never the secret). It does not assert the side effect. An observation artifact never
implies an enforcement, and this record never implies a side effect.

## 12. Failure semantics (extends P61a)

- PDP cannot decide → `proxy_denied` (fail-closed), never forwarded;
- upstream unreachable on an *allowed* call → `proxy_failed` (the decision stands; the forward failed);
- malformed upstream response → never trusted (as P61a);
- the enforcement record is written for both allow and deny; a record-write failure on a requested
  output path is a non-zero exit, not a silent allow.

## 13. What enforcing-v0 is NOT

No token minting / OAuth authorization-server behavior; no sandboxing of the upstream; no behavior
verification; no descriptor semantic analysis; no maliciousness classification; no LLM judge; no
HTTP-upstream or multi-upstream (still stdio, one upstream, as P61a); no change to the manifest-
observation artifact shape.

## 14. PR slicing

```
P61e-a  this review-spec (design doc, no code)
P61e-b  enforcing run mode + PDP skeleton that is DENY-ALL fail-closed (no allow path yet) +
        proxy_denied wired + the negative test from day one: a privileged tools/call is denied,
        the upstream receives nothing, the client gets proxy_denied
P61e-c  the allow path: per-caller authorization (P58 declared allowance) + credential-scope gate
        (P59) + the drift-aware gate (P60/E12) — each gate fail-closed and independently tested
P61e-d  the assay.enforcement_decision.v0 record + the side-effect-evidence interaction (asserted),
        kept separate from the observation artifact; Plimsoll consumes it separately (later)
```
P61e-b lands the enforcement boundary (deny-all) before any allow path exists, so "fail-closed" is
testable the moment the mode exists — the same discipline as P61b's negative-forwarding test.

## 15. Design rules (binding for the arc)

- enforcing mode is explicit opt-in; the default stays manifest-observation;
- the PDP runs before forwarding; an unclear decision is a deny;
- ambient (proxy) authority never becomes caller authority; per-caller allowance required;
- no token passthrough; no broadening of caller authority; upstream credential by alias only;
- a privileged call into a tool that drifted since approval is denied or pending, never silently allowed;
- a forwarded call's side effect stays asserted; allowing is not proving;
- `proxy_denied` is distinguishable from upstream/`proxy_failed`/`proxy_unsupported`, with a machine reason;
- enforcement evidence is a separate carrier from observation evidence.

## 16. Open questions

- **A. Mode shape:** a separate enforcing subcommand vs an `--enforce` flag on the proxy — which fits
  the CLI conventions? (Either way: explicit, never implicit.)
- **B. Drift baseline source:** does the PDP take the approval-time baseline from a declared artifact
  (the P60d `assay.declared_mcp_manifest.v0`), from the first observed complete manifest of the
  session, or both — and is a session with no complete approval-time observation allowed to enforce at
  all (my lean: no — fail-closed, you cannot enforce drift against a baseline you never established)?
- **C. Review-mode vs hard-deny:** on drift / missing-allowance, is the default `proxy_denied`
  (hard) or `pending_tool_manifest_review` (hold for human review)? My lean: hard-deny by default,
  review-mode as an explicit policy opt-in.
- **D. Caller identity source:** over stdio there is no transport identity — is the "caller" the single
  client of the stdio session (one caller per process), with multi-caller deferred? My lean: yes,
  one caller per stdio session in v0.
- **E. Scope of v0:** confirm v0 is stdio + one upstream + deny-all-then-narrow-allow, with token
  minting / OAuth resource-server behavior explicitly out.

My one-line recommendation: **explicit opt-in enforcing mode, fail-closed deny-all first (P61e-b),
then narrow allow gated by per-caller allowance + credential-scope + drift-since-approval, with a
separate enforcement-decision record and the side effect kept asserted.** Mark this up and I'll fold in
the decisions before any code.

[#1624]: https://github.com/Rul1an/assay/issues/1624
