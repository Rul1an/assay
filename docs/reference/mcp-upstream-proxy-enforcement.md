# MCP upstream proxy — enforcing `tools/call` mode (P61e review-spec)

Status: **review-spec, no code.** A new arc and the heaviest risk class in the proxy line. It extends
the shipped, opt-in [manifest-observation proxy](mcp-upstream-proxy-mode.md) (P61a–d, assay v3.23.0)
from observe-only to **enforcing**: it forwards a privileged `tools/call` only after a fail-closed
policy decision. Tracked as [#1624]. The binding decisions are now agreed (§16, "Resolved decisions");
this is the design of record. No code lands in this doc — P61e-b (a deny-all enforcing mode) is the
next slice — because forwarding privileged calls with an upstream credential makes the proxy a confused
deputy unless every dimension here is locked first.

The one-line scope: **the manifest-observation proxy answers "did this tool surface change?"; the
enforcing proxy answers "should this specific privileged call be forwarded, right now, for this
caller?" — and defaults to no.**

The review sentence to keep: *P61e turns tool-call forwarding into an enforcing, fail-closed decision
point. It does not make the proxy an authorization server, does not prove side effects, and does not
allow observe-only privileged forwarding.*

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
explicit opt-in run mode** — not a flag on the same proxy, because a flag reads as a small variation
and this is a different risk class.

**Mode shape (decided): a distinct enforcing subcommand.**
```
assay-mcp-server proxy observe ...   # manifest-observation (the shipped P61a–d mode)
assay-mcp-server proxy enforce ...   # enforcing tools/call (P61e)
```
Nested `proxy observe` / `proxy enforce` subcommands; if the clap layout makes nesting awkward, two
top-level subcommands `proxy-observe` / `proxy-enforce`. Either way the enforcing mode is its own
command, never implicit. A deployment that does not run the enforcing mode keeps P61b–d's behavior
(every `tools/call` → `proxy_unsupported`).

**Implementation note (v0, P61e-b):** the shipped CLI spelling is the top-level **`proxy-enforce`**
subcommand (a sibling of the existing `proxy`), chosen so the shipped `proxy --upstream-command …`
invocation is untouched and the enforcing slice stays the smallest possible change in a
security-load-bearing PR. The design concept is unchanged — an explicit, separate enforcing proxy mode;
folding it into nested `proxy observe` / `proxy enforce` is a later ergonomic change, not a v0 concern.

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

**The PDP runs on every `tools/call`, not only on privileged-classifiable calls** — "unclassified" is
not "safe", so an unclassifiable call is exactly the fail-closed case. The decision logic:

- the call is classified privileged (P57c) → evaluate the privileged policy gates (caller allowance,
  credential scope, drift) and forward only on a clear allow;
- the call is unclassified, or classification is incomplete → **deny** unless an explicit
  non-privileged allow rule matches; an unclassified call never passes through by default.

Non-`tools/call` methods keep the manifest-observation behavior (handshake, `ping`, `tools/list`);
everything else stays `proxy_unsupported`. For P61e-b (below) the whole `tools/call` surface is
deny-all; the allow path arrives only in P61e-c.

## 5. Caller authorization (the confused-deputy core)

The decision to forward a privileged call must consider **who is calling** and **what they are
allowed to do** — not merely "the proxy can reach the upstream."

- **Caller identity is declared, not inferred (decided).** Over stdio there is no transport identity,
  so v0 is **one caller per stdio session**, declared via explicit config — never inferred from the
  transport, no multi-caller, no OAuth, no token introspection:
  ```yaml
  caller:
    id: "ci-agent"
  ```
  A privileged call with no declared caller id → `proxy_denied` (`unknown_caller`). Multi-caller is a
  later arc.
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
machine reason from the pinned set:
```
unknown_caller                          classification_incomplete
unclassified_tool_call                  no_declared_allowance
credential_scope_insufficient           credential_scope_unknown
manifest_baseline_missing               manifest_current_observation_incomplete
manifest_drifted_since_approval         policy_unavailable
policy_error                            enforcing_mode_deny_all   (P61e-b only)
```
A denied call never reaches the upstream. **P61e-b** uses exactly one reason — `enforcing_mode_deny_all`
(the whole `tools/call` surface is denied before any allow path exists); the gate-specific reasons
arrive with the gates in P61e-c. `credential_scope_unknown` is distinct from
`credential_scope_insufficient`: when coverage cannot be determined the denial reason is *unknown*, not
*insufficient* (see §8).

## 8. Credential boundary (locked from P61a, extended)

- **No token passthrough by default.** Inbound caller auth is never forwarded to the upstream
  (the standing no-passthrough invariant from P61b).
- **Upstream credential from operator config only**, referenced by alias in evidence, never by value;
  raw key material is never stored.
- **Least privilege on the upstream credential** (ties to P59 credential-scope): the proxy should hold
  only the scope the deployment legitimately needs, and a call requiring more than the declared
  credential scope covers is denied (`credential_scope_insufficient`). **Hard non-claim: declared
  credential scope coverage is policy metadata, not a provider-verified token grant** — there is no
  token introspection. When coverage cannot be determined, the call is denied with
  `credential_scope_unknown`, never silently `credential_scope_insufficient` (an unknown is not an
  insufficiency). This gate is P61e-c.
- **Audience binding (concept-aligned, RFC 8707 / OAuth 2.1).** The proxy must not let a token or
  credential issued for one audience be borrowed to reach another; it does not upgrade or broaden
  caller authority. v0 need not implement token minting, but it must state and enforce that the proxy
  does not broaden authority.

## 9. Drift-aware enforcement (the novel contribution)

Enforcement is tied to the manifest-drift state, connecting P60 (manifest drift), E12 (tool lifecycle),
and P61e (enforcement): a privileged call to a tool whose **contract digest changed since the caller
approved it** is invoking something other than what was approved — the post-approval-mutation payoff.
The drift gate (P61e-c) needs **both** of two evidence inputs, and is fail-closed without either:

- **the approval-time baseline comes from a declared approved manifest artifact only** — the
  `assay.declared_mcp_manifest.v0` baseline (or a future approval record that references it), **never
  the first observed complete manifest of the session**. A first-observed baseline can already be a
  post-rug-pull state, so pinning it and calling it "approval-time" would pin the wrong state. No
  declared approved baseline → `proxy_denied` (`manifest_baseline_missing`);
- **a current complete observed manifest is required** — if the session has no complete current
  observation of the invoked tool's surface, the drift state is `proxy_denied`
  (`manifest_current_observation_incomplete`), because you cannot compare against a baseline without a
  current complete view.

With both in hand:
- current contract digest **equals** the declared approved baseline → the drift gate is satisfied
  (other gates still apply);
- digest **drifted** since approval → `proxy_denied` (`manifest_drifted_since_approval`) — a hard deny;
- drift state **inconclusive** (partial/unobserved, per the E12 boundary) → fail-closed deny, because
  "no drift observed" is not "no drift."

`pending_tool_manifest_review` is review-layer language, not a runtime proxy response; v0 is hard-deny
(see §16-C). The proxy checks runtime manifest evidence on the call path instead of relying solely on
prior review.

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
credential alias (never the secret). It does not assert the side effect.

**Deferred to P61e-d** — the record is not in P61e-b. P61e-b proves runtime *denial semantics* first;
the per-call record follows once those semantics are settled.

**Name discipline (no overlap with the existing carrier):**
- `assay.enforcement_health.v0` = mechanism / runtime-capability state (was enforcement active, did the
  mechanism attach) — the existing carrier;
- `assay.enforcement_decision.v0` = the per-call proxy policy decision (allow/deny + reason) — this new
  one. They never overlap: one is "could the mechanism enforce", the other is "what did the proxy
  decide for this call".

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
P61e-b  the enforcing run mode exists and is DENY-ALL: every tools/call -> proxy_denied
        (enforcing_mode_deny_all), the upstream receives nothing, the allowlisted list methods still
        forward, an unknown method is proxy_unsupported. NO PDP skeleton, NO inputs, NO allow path.
P61e-c  the allow path + PDP: caller identity (config) + per-caller authorization (P58 declared
        allowance) + credential-scope gate (P59) + the drift-aware gate (declared baseline + current
        complete manifest, P60/E12) — every gate fail-closed and independently tested
P61e-d  the assay.enforcement_decision.v0 record + the side-effect-evidence interaction (asserted),
        kept separate from the observation artifact; Plimsoll consumes it separately (later)
```
**P61e-b is deliberately just a deny-all enforcing proxy** — no policy decision point, no inputs, no
allow path. It lands the enforcement *boundary* so "fail-closed" and the `proxy_denied`-vs-
`proxy_unsupported` distinction are testable the moment the mode exists (the same discipline as P61b's
negative-forwarding test), without building any of the gate machinery yet.

P61e-b tests (mirror P61b's negative-first discipline):
```
proxy enforce: a tools/call -> proxy_denied (enforcing_mode_deny_all); the upstream receives nothing
proxy enforce: an unknown method -> proxy_unsupported (not proxy_denied)
proxy enforce: an allowlisted list method (tools/list) still forwards
proxy observe: a tools/call still -> proxy_unsupported (the modes stay distinct)
```
This keeps `proxy_denied` (policy denial, enforcing mode) cleanly separate from `proxy_unsupported`
(method not handled).

## 15. Design rules (binding for the arc)

- enforcing mode is explicit opt-in; the default stays manifest-observation;
- the PDP runs before forwarding; an unclear decision is a deny;
- ambient (proxy) authority never becomes caller authority; per-caller allowance required;
- no token passthrough; no broadening of caller authority; upstream credential by alias only;
- a privileged call into a tool that drifted since approval is denied (hard-deny in v0; review-mode is
  a later, explicit opt-in), never silently allowed, and the baseline is a declared approved manifest;
- the PDP runs on every `tools/call`; an unclassified call is denied, never passed through by default;
- a forwarded call's side effect stays asserted; allowing is not proving;
- `proxy_denied` is distinguishable from upstream/`proxy_failed`/`proxy_unsupported`, with a machine reason;
- enforcement evidence is a separate carrier from observation evidence.

## 16. Resolved decisions

- **A. Mode shape — decided:** a distinct enforcing subcommand (`proxy enforce`, alongside
  `proxy observe`), never an `--enforce` flag on the same proxy. A flag reads as a small variation;
  this is a different risk class (§1).
- **B. Drift baseline source — decided:** the approval-time baseline comes from a **declared approved
  manifest artifact only** (`assay.declared_mcp_manifest.v0`, or a future approval record referencing
  it), never the first observed session manifest. Enforcement also requires a **current complete
  observed manifest**; without either, the drift gate is a fail-closed deny
  (`manifest_baseline_missing` / `manifest_current_observation_incomplete`). You cannot enforce drift
  against a baseline you never established (§9).
- **C. Review-mode vs hard-deny — decided:** **hard-deny by default.** `pending_tool_manifest_review`
  is review-layer language and is not a runtime proxy response in v0; the proxy is on the call path and
  must return a concrete decision. A `--decision-mode review` is a later, explicit opt-in, not v0 (§9).
- **D. Caller identity source — decided:** **one caller per stdio session, declared via explicit
  config** (`caller.id`); no inferred transport identity, no multi-caller, no OAuth, no token
  introspection. Missing caller → `proxy_denied` (`unknown_caller`) (§5).
- **E. Scope of v0 — decided:** stdio + one upstream + deny-all (P61e-b) then narrow allow (P61e-c);
  no token minting, no OAuth resource-server behavior, no HTTP upstream, no multi-upstream (§13).

One-line recommendation, now locked: **explicit opt-in `proxy enforce` subcommand, fail-closed
deny-all first (P61e-b), then a narrow allow gated by declared caller allowance + credential-scope +
drift-since-approval against a declared approved baseline with a required current complete manifest, a
separate enforcement-decision record (P61e-d), and the side effect kept asserted.** P61e-a (this spec)
is approved; P61e-b code is the next step, not part of this doc.

[#1624]: https://github.com/Rul1an/assay/issues/1624
