# PLAN - P58 MCP Declared-vs-Observed Capability Evidence (2026 Q2)

- **Date:** 2026-06-03
- **Owner:** Evidence / MCP Security
- **Status:** Watchlist sketch (NOT planning-to-implement, NOT a roadmap)
- **Scope (this note):** Hold a durable seam idea for comparing a *declared*
  MCP server-capability surface against Assay's *observed* `capability_surface.v0`.
  This is a watchlist item, gated on a real, sourced metadata surface appearing.
  No implementation, no schema freeze, no public outreach.

## 0. Provenance caveat and naming

This sketch was prompted by secondary search summaries that referenced a
hypothetical MCP "server metadata / card" surface, plus spec names, SEP numbers,
and a release date that were **forward-dated relative to today (2026-06-03) and
not independently verifiable**.

Therefore:

- "Server Card" in this document is a **working placeholder** for a hypothetical
  MCP server-metadata surface. It is **not** asserted as a confirmed MCP feature.
- No spec name, SEP number, or release date is stated as fact here.
- Nothing in this sketch should be cited publicly as upcoming MCP behaviour
  until a primary, dated source is in hand.

The durable thesis below does **not** depend on any of those unverified names.
It depends only on local, verified Assay building blocks.

## 1. The durable thesis

Whatever form a future MCP server-metadata surface takes, it will be a
**declaration**: "this server offers these tools / resources / prompts." A
declaration is a claim, not a verified fact.

Assay's adjacent role is narrow and well-matched:

> Compare a declared server-capability surface against the observed
> `capability_surface.v0`, keep the two as distinct comparison buckets mapped
> onto existing claim-class cells, and report the diff. Do not treat a
> declaration as truth, and do not certify servers.

This fits because Assay already has the local building blocks (all verified
2026-06-03):

- `capability_surface.v0` (observed sets: tools, network, filesystem, process,
  policy) - `crates/assay-runner-schema` (`CAPABILITY_SURFACE_SCHEMA`).
- claim-class discipline - `docs/reference/observability/claim-classes-v0.md`.
- declared-rule path-projection helper - PR #1469 (declared rules kept distinct
  from observed evidence).
- observed-only capability diff boundaries -
  `docs/reference/runner/capability-diff-v0.md` and
  `docs/reference/runner/cross-runtime-diff-v0.md` both keep
  declared-capability input out of their v0 contracts.
- runner artifact contract - `docs/reference/runner/artifacts-v0.md`.

So the reputational line is plausible and bounded: not "we certify MCP servers",
but "we compare declared capability surfaces with observable facts and keep claim
boundaries explicit".

## 2. Trigger condition (what flips this off the watchlist)

Promote from watchlist to a real plan only when a **primary, dated source**
shows a concrete declared-capability surface that can be frozen locally - for
example a published metadata document, a `.well-known`-style endpoint, or a
registry record - with a stable shape. Until then this stays a sketch.

## 3. The conditional seam

IF such a declared-capability surface exists and can be frozen, THEN:

- treat the declaration as a `declared_capability` comparison bucket or future
  `claim_type`, separate from the `observed_capability` bucket derived from
  `capability_surface.v0`;
- diff the two: declared-only, observed-only, agreed;
- attach claim-class cells per side rather than inventing a new vocabulary:
  declaration uses a reported/asserted basis, while observed surface uses the
  measured or derived basis supported by the runner artifact;
- report the diff without promoting either side into proof.

The output is a comparison artifact, not a verdict on server trustworthiness.
A declared tool that is never observed is not automatically "wrong" (it may be
unused in the run); an observed tool that was never declared is the more
interesting signal. Both are reported, neither is a certification.

The comparison also needs a server-scope join. Today
`capability_surface.v0` is run-global: it captures observed tools, network,
filesystem, process, and policy sets for the run, not a per-server capability
view. A graduated P58 fixture must therefore scope the observed surface to the
declared MCP server, or join it through a stable server/upstream identity such
as a P57 `upstream.target_digest`, a declared metadata digest, or an explicit
single-server fixture boundary. Without that join, an observed-only tool could
belong to another MCP server or runtime surface.

## 4. Negative fixtures (when it graduates)

1. **Declared tool missing from observed** - declared `deploy_service`, never
   appears in `capability_surface.v0`; expected: reported as declared-only, not
   a hard failure on its own.
2. **Observed tool undeclared** - a tool shows up in observed capability with no
   matching declaration; expected: surfaced as observed-only (the higher-signal
   case).
3. **Stale declaration** - declaration digest does not match the current server
   build; expected: flagged as stale, comparison marked lower-confidence.
4. **Overbroad capability claim** - declaration asserts capabilities far beyond
   anything observed; expected: reported as declared-only breadth, never read as
   proof the server can/will do them.
5. **Unknown mapping** - a declared entry cannot be classified into a known
   capability category; expected: `unknown` kept distinct from absent.
6. **Declaration treated as verified truth** - artifact drops claim classes and
   asserts the declared surface as fact; expected: lint failure.

## 5. Relation to P57

P57 is the **observed** transport/route side of a tunneled MCP request. P58 is
the **declared-vs-observed capability** side. They compose: P57 observes how a
request reached the server; P58 (if it graduates) compares what the server
*claims* to offer against what was *measured*. The diff is the differentiated
play - but only P57 is concrete today.

## 6. Promotion criteria

Promote only when all are true:

1. A primary, dated source defines a freezable declared-capability surface.
2. The declaration can be frozen locally without raw secrets or payloads.
3. The observed surface is scoped to the declared server by a stable
   server/upstream identity or an explicit single-server fixture boundary.
4. The diff keeps `declared_capability` and `observed_capability` as separate
   comparison buckets or claim types, mapped onto the existing claim-class cell
   vocabulary rather than replacing it.
5. At least two negative fixtures from section 4 are covered.
6. The docs can state, in one sentence, what Assay does **not** prove (it does
   not certify the server or verify the declaration as truth).

## 7. Stop lines

Do not proceed if the work requires:

- citing unverified spec names, SEP numbers, or release dates as fact;
- treating a declaration as authorization, identity, or runtime truth;
- certifying or scoring MCP servers;
- provider-specific code before a provider-neutral, sourced declaration fixture
  exists.

## 8. Outreach posture

Do not open a public thread about this until a primary source exists. When a
real metadata surface is published and a boundary question arises, the bounded
line is:

> If MCP gains a server-capability declaration surface, the useful split is
> declared capability vs observed capability. A declaration can be compared to
> measured facts and mapped to an explicit claim-class cell - it should not be
> read as proof that the server is trustworthy or that the capability was
> exercised.

Never state, publicly, that a specific declared-capability feature is shipping
in a specific release without a dated primary source.
