# MCP era-parity corpus, v0

**Maturity: exploratory. Lower than `conformance/privileged-mcp-action-v0/`, and deliberately so.**

That corpus is frozen, digest-pinned and carries an open reproduction request. This one is a working
set for slice 1 of #1866: it changes as vectors are added, it has no reproduction request, and no
claim made here inherits the frozen corpus's standing. Two corpora side by side without an explicit
maturity line let a reader borrow the older one's credibility, so the line is stated first.

## Layers, and why they are separate

`MANIFEST.json` is the table. Each vector states its expectations at layers that are never collapsed
into one verdict, because they disagree on purpose.

| Layer | Question it answers |
|---|---|
| `schema` | does the pinned artifact for *this vector's era* accept *this message*? |
| `observation` | what did the transcript carry? |
| `conclusion` | what does that observation license under the resolved era? |
| `profile_baseline` | the frozen row the surrounding evidence supplies, referenced and never restated |

Every layer is executable. Each label is decoded into a value and compared against what the parser
actually produced, and an unrecognised label fails rather than being skipped, so a row cannot drift
away from the enum by being renamed. Observation is pinned as well as conclusion: a right conclusion
reached from a wrongly observed input is a passing test that proves nothing.

The load-bearing case is a **pair**: `complete-with-input-requests` and
`complete-with-request-state`. The published `CallToolResult` definition sets no
`additionalProperties: false` and lists neither member, so it accepts a result claiming completion
while carrying either one — and Assay must not conclude terminal for either. A schema verdict is
evidence about bytes, never the semantic oracle.

The two exist separately because of what `InputRequiredResult` says versus what it encodes. Its
description states that *at least one of* `inputRequests` or `requestState` MUST be present; its
`required` list contains only `resultType`. The MUST lives in prose, so the vendored artifact
accepts a result that violates it, and no schema verdict can report the violation.

That gap is why the conclusion layer reads both members itself, and why the negative vectors exist:
`input-required-without-continuation` and its explicit-null twin are accepted by the artifact and
refused by the conclusion. Both members carry a call forward, so a completion claim beside either is
the same contradiction, and a rule reading only the first leaves a transcript able to state it in a
shape the build cannot see. One vector per member means a change that stops reading either one has
somewhere to fail; a single vector covering both at once would keep passing on the half still read.

### Per-call rows

A vector states its conclusion once, either as one `conclusion` for every hop or as a `per_call`
list, never both: two sources of truth can disagree silently.

`per-request-capabilities-interleaved` is the reason `per_call` exists. Capabilities are stated per
request and MUST NOT be inferred from a prior one, so that vector sends two calls on distinct ids
advertising different capability sets and answers them in reverse order. Both responses carry the
*same* unrecognized token, which is what makes it load-bearing: nothing in the response bytes can
separate the two conclusions, so only the binding to each response's own request can. The token is
also identical to the advertised extension name, the shape most likely to tempt an invented mapping
from extension to token.

Its rows are addressed by correlation id rather than by reading position, and the test asserts the
arrival order really is reversed before asserting anything else — otherwise it would prove nothing.
Two cheap implementations are ruled out by the pair of exact expectations: transcript-global state
gives one answer to both responses, and last-seen state binds the second request's capabilities to
both. Each fails a different row.

### The schema layer is per message and per era

Two slots, named for what is actually validated:

- `request_message` — the whole JSON-RPC request object.
- `result_payload` — the `result` object only, not the enclosing response message.

Every hop is validated, not only the first, so a multi-hop vector cannot hide a second message behind
a passing first one.

Two vectors state **no** schema verdict. `unknown-era` and `conflicting-era-signals` leave the era
undetermined, and which artifact applies is exactly what is undetermined.

For `conflicting-era-signals` the disagreement is narrower than the ambiguity suggests, and worth
stating precisely because the corpus once said otherwise. The request validates under *both*
artifacts: `2025-06-18` does not forbid the extra `_meta` members, and the request carries exactly
what `2026-07-28` requires. Only the result flips — the legacy `CallToolResult` requires only
`content`, the modern one also requires `resultType`. Reporting one boolean would still mean
choosing a side the transcript does not license, which is the fault the vector exists to catch, and
`the_conflicting_vector_flips_only_on_the_result` pins all four outcomes so the description cannot
drift from the artifacts again.

## Schema pins

`PIN.json` records the spec commit and a sha256 per vendored artifact. One artifact per era a vector
actually declares: the legacy vector carries `2025-06-18`, so that is what it is validated against.
Validating it against a neighbouring revision would answer a question no vector asks. In `2025-06-18`
`CallToolResult` requires only `content`, which is why an absent `resultType` is valid there rather
than merely tolerated.

The schemas are vendored rather than fetched, so the corpus runs offline and a silent upstream edit
cannot change what a vector means. The digests are checked before the schemas are used for anything.

The two artifacts are written against different drafts, which is not cosmetic: `2026-07-28` is
2020-12 and keeps its definitions under `$defs`, `2025-06-18` is draft-07 and uses `definitions`.
The container is chosen from what the artifact actually carries, and an artifact carrying both is
refused rather than resolved — which one a `$ref` should reach would be exactly what is unclear.

## The profile baseline

Every vector references the frozen row `ok-003-allow-no-outcome-observation` and none restates its
matrix, so the two cannot drift apart.

Read that reference precisely. It is a **baseline supplied by the surrounding producer-reported
policy evidence, not a profile derived from these bytes.** A wire transcript cannot confirm
`policy_decision_recorded`: that cell rests on Assay's own decision record, which is not in the
fixture. `caller_visible_denial` is not inferable from a transcript either. What these vectors can do
is fail to move `upstream_delivery` and `external_side_effect`, and the row keeps all three
incomplete for exactly that reason.

No vector introduces a profile term, a generic verdict, a scalar score, or a conformance claim.
