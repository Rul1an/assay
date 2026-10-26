# Bounded A/B/C evidence comparison contract

Date: 2026-10-26. Status: **contract only; execution HOLD**.

This narrows the next diagnostic comparison under the
[original design, section 6](2026-10-22-agent-session-evidence-design.md#6-preregistered-comparison-and-acceptance).
It does not replace that design's acceptance criteria or authorize a capture,
package projection, receiver experiment, production source adapter, or release.
No schema, identifier, predicate, CLI output or exit-code contract changes.

## Subject and present evidence

The sole proposed subject is retained capture
`capture-receipts-v3-real-112e1fbb2b9941138f51f1832d1a5863`.
Original archive SHA256:
`7950a6273a23afd063a04684cae4e2e731034a5a3e7eea81aa0ace6ab815185f`.
It contains 46 files / 69,243 original bytes. Never combine it with earlier
attempts or repair its originals. This is a retrospective diagnostic contract,
not a claim that its questions were blinded to the designers before capture.

Proxy source: `6f77712128d3cf81ab85095e3bd907122809c65c`; experimental source-build
SHA256 `4aa00c9f2d240b24ac67bdca9f7203076bf1b7f074ef0a212d9e6764515af31a`.
Task-local wrapper SHA256:
`8598669cb9a948ab3f5c37e42b74e99341429fbf0d4bbaec08de4a39b97805fc`.
These identify the measured implementation, not a released installation.

An independent packet-only replay recomputed native stop `closed`, receipt
closeout `complete`, and three exact request/decision byte associations. A
JSON-valid decision-byte change was refused. This replay omitted vendor records
and kernel evidence; it was not A, B or C, nor a fresh blind recipient study.
The producer reports EOF following SIGINT, exit0 and successful cleanup. A later
live check found its capture cgroup absent. Offline recipients can validate
recorded cleanup fields, not independently observe historical OS cleanup.
The kernel archive is **present_unverified**.

## Views and common material

All views concern this same capture. Do not run three agents or replay actions.
Source availability is nested: A is a subset of B, B a subset of C.

| View | Permitted source content | Current gate |
| --- | --- | --- |
| A | Original supported vendor events and hook occurrences, documented native inspection route, relevant capture settings | Source attribution and byte-preserving packaging not yet established; keyed hook lifecycle reconciliation unavailable for this capture |
| B | A plus original proxy decisions, request receipts, session context and native stop; applicable policy/mode configuration and necessary proxy diagnostics | Earlier proxy subset replay passes; B excludes supervisor and cannot reproduce full receipt closeout; combined A+B view not yet built or tested |
| C | B plus original kernel evidence and its capture/health provenance | Blocked until pinned canonical archive verification and health/coverage eligibility pass |

Each view gets its own exact-membership manifest, externally selected expected
digest, passive readers/instructions and source-specific provenance. Identical
common material contains only scope, questions, bounds and integrity metadata.
It must not disclose richer-view answers or B/C-only file names, counts and hashes to A: no shared supervisor summary containing
B's stop/receipt verdicts, no shared kernel-health conclusions. Retain the full
original supervisor privately. It contains C-only capture metadata and producer
stop/receipt verdict summaries, so neither A nor B receives it, an excerpt, or a
rewritten replacement. B may independently run native stop verification over
its original stop, context and decision records. Full receipt closeout requires
supervisor lifecycle inputs and is **unavailable in B**; the earlier packet-only
replay does not transfer that result to B. C or the separate adjudicator may run
the existing closeout method only with its original inputs and all dependencies
satisfied. Producer verdict fields never substitute for recomputation; private
adjudication never becomes a recipient's answer. The same leak boundary applies
to mixed `provenance.json`: keep the original outside A/B, never relabel a filtered
copy as original. Necessary source-local pins may appear as explicitly derived
packaging metadata, without richer-view fields or verdicts. If the necessary
provenance cannot be supplied within that boundary, stop packaging.

### Mandatory packaging gates and unavailable evidence

Source/version attribution, exact-byte integrity and membership, bounded passive
readers, credential screening and absence of richer-view leaks are mandatory
for every packaged view. A failed or unavailable mandatory gate stops that view;
B cannot proceed without an eligible A, and C cannot proceed without eligible B
and kernel verification. No diagnostic label waives those gates.

For this capture, vendor stream hook lifecycle events carry `hook_id` while raw
hooks carry `tool_use_id`. The existing
[`record_hook.reconcile`](../../../examples/agent-session-evidence-probe/record_hook.py)
requires native `(hook_event_name, tool_use_id)` keys. Its check is unavailable,
not passed. Counts, order, timestamps, or IDs borrowed from assistant/user
messages cannot fill the missing lifecycle keys. Keep all original identifiers
and namespaces unchanged; do not inject a mapping or claim complete hook capture.

This clarification permits only a future, separately authorized diagnostic view
with that missing lifecycle evidence explicitly declared. It does not satisfy or
relax the original design's lifecycle or full-acceptance requirements. Keep all
six question rows: reported tool/input and the predeclared configuration-inspection
decision may use eligible vendor bytes; proxy decisions may use B's eligible
bytes. Hook completeness, native cross-source identity, and any proposition
requiring the missing binding remain unavailable/unresolved. Host observations
remain unavailable until C passes its gates. Offline reproducibility covers only
the answers and methods actually available in that view. Missing evidence is
neither a negative answer nor a failed vendor capability.

`native.stdout` mixes runner and vendor material. Do not give A that entire stream
or reserialize selected JSON as original bytes. Before packaging, freeze an
explicit source-attribution rule tied to pinned vendor event shapes and exact
runner output templates, exact ordered byte ranges, source digest and
projection digest; account for every original byte exactly once (including runner framing) and
require each range to match exactly one permitted source format without inferring
from the expected answer. A JSON object with a `type` field alone is insufficient.
Unknown, ambiguous or double-matching ranges stop packaging. Format consistency
is not producer authentication or proof of atomic writes. Preserve the original stream outside recipient views. If this
cannot be established, stop packaging and record the ambiguity. The complete range accounting and mixed-source digest remain with the
adjudicator; each recipient receives only its eligible excerpts, their hashes
and source-local navigation. Derived maps
are navigation metadata, never new source events or identity evidence.

### Capture-specific disposition of vendor extension fields

For this capture only, the source-format rule may retain the following 14
observed top-level fields as **opaque extensions** to documented vendor envelopes.
Their presence alone does not fail attribution; it does not establish their meaning.
The independent envelope basis is `@anthropic-ai/claude-agent-sdk@0.3.308`, whose
package declares `claudeCodeVersion: 2.1.308`; the retained `sdk.d.ts` SHA256 is
`0502b6e4f627dc199f12222c7214d36b716a9e80d0f2cb7704cd235d43a680a8`.
This disposition is bound to `native.stdout` SHA256
`e054025bf9ae14a90eb81267cfa7140f4f9762fd044591129e99cdd9e7fabd98`.
The binary/version binding and registry-integrity evidence remain separately
required. Neither this reference nor the observed inventory authenticates stdout.

| Envelope | Opaque top-level fields (observed JSON kind) |
| --- | --- |
| `assistant` | `thinking_display` (string), `thinking_duration_ms` (integer), `tool_use_meta` (array), `wire_tool_inputs` (object) |
| `user` | `tool_result_meta` (array) |
| `system/init` | `additional_directories` (array), `analytics_disabled` (boolean), `memory_paths` (object), `messaging_socket_path` (string), `original_cwd` (string), `per_turn_effort_active` (boolean), `product_feedback_disabled` (boolean) |
| `result/success` | `safety_stops` (integer), `subagent_stats` (object) |

Here, integer means a JSON number token without a fraction or exponent, matching
the existing parser; booleans are a separate kind. The `user` envelope uses
`SDKUserMessage`, not `SDKUserMessageReplay`; this capture has no `isReplay` field.

This is a closed, capture-specific list, not a general unknown-field allowance.
Retain the exact per-record shape variants from the frozen attribution profile
(SHA256 `9bd36123772dfd38531f813430f880a44d9293f433d16124f696d49507c22c15`);
the table does not permit arbitrary combinations, optionalization or new fields.
Check documented discriminators and required envelope fields against the pinned
vendor declarations, then the exact observed key sets and JSON kinds. Retain all
existing duplicate-key, encoding, size, depth and occurrence checks, including
inside opaque values. Unknown variants, changed kinds, attribution ambiguity or
a second source-format match still stop packaging. The full profile includes
runner material and remains with the adjudicator, outside A; it is not common
recipient metadata. This disposition is not a new executable validator.

Preserve each complete original line, including these values and its framing;
do not strip, normalize, rename, reserialize or synthesize any field. Opaque
extensions may be retained and navigated as bytes, but cannot supply an answer,
verdict, identity, causal join, completeness or safety claim. In particular,
`wire_tool_inputs` cannot replace or repair the documented tool-input path, and
`tool_use_meta`/`tool_result_meta` cannot repair hook lifecycle keys. Do not pick
whichever representation supports an answer; unresolved input ambiguity leaves
the affected claim unresolved. Documented fields support only claims within
their declared meaning and existing evidence requirements. This top-level
disposition does not validate nested message semantics or establish support for
a claim about tool-input blocks merely because the envelope matches. No complete vendor
schema, exact embedded API-SDK version or reproducible build becomes a new gate.

Opaque does not mean safe to share: screen all original bytes, including opaque
values, account-usage records and paths. Unsafe content stops packaging; this
rule permits no silent redaction or omission. All identifiers, namespaces and
unavailable-evidence boundaries above remain unchanged. The actual passive reader,
source-local provenance, exact package membership, externally selected expected
digest and leak/integrity checks still need verification on the final artifact.
This resolves only the extension-field disposition; it admits no package and
authorizes no projection or recipient experiment. **A/B/C execution remains HOLD.**

A receives a documented offline native inspection route; unsupported Assay
parsing must not be scored as a vendor-evidence failure. Leak checks must distinguish added B/C-only metadata from genuine vendor content
that happens to mention a proxy, its denial, or a configured wrapper path.
Do not censor the latter or reject a legitimate vendor record solely because
its text contains a token such as `session-stop`.
No vendor login, model,
network, original checkout, oracle or other view's answers are available to a
recipient. Settings/provenance are screened for credentials and richer-source
leaks before any sharing. An unsafe occurrence blocks packaging; it is not
silently redacted and then called original.

## Frozen questions and answer rows

Use these six original questions for every view. Record an answer, exact
file/byte or existing-record reference, inspection method, admissibility limit,
uncertainty and receiver friction. Separate unavailable evidence from evidence
of a negative. Preserve canonical verifier states rather than inventing a new
aggregate verdict or scalar trust score.

| Question | Admissible basis | Prohibited inference |
| --- | --- | --- |
| What tool/input did the runtime report? | Original vendor record and exact reference | Report equals execution; truncated argument equals exact input |
| What decision did this proxy record? | Decision bytes and applicable policy/mode | Allow equals execution success; unknown mode equals enforcement |
| Is the vendor call the same as this proxy call? | Existing admitted native binding, uniqueness and window conditions | Caller-supplied metadata, ordering or timestamps alone establish native identity |
| Which host operations were retained? | Positive verified kernel record with eligible provenance/health | Missing event proves no activity; event proves per-tool causality |
| Does a specific proposition contradict retained evidence? | Existing predicate and its required binding/coverage | Missing source alone proves contradiction; diagnostic relation becomes admitted predicate |
| Can an offline receiver reproduce the answer? | Exact package, pinned passive method, independent expected digest and fresh results | Producer summary or process exit0 alone proves validation |

The request receipts support only the existing caller-asserted request-to-decision
byte association. Keep event IDs, context IDs, receipt scope/sequence, typed RPC
IDs and vendor IDs unchanged and in their existing namespaces. No injected ID,
deduplication, timestamp backfill, cross-source identity or causal join is added.
A unique receipt match does not answer the stronger native-identity question.
`closed` and `complete` retain their existing limited meanings; neither is a
whole-action verdict, authenticity, global completeness or durable-storage proof.

## Predeclared decision and bounded method

The investigation decision is: **for the reported `policy_denied` attempt,
should an investigator inspect MCP policy configuration, vendor permission
configuration, or leave the responsible layer unresolved?** Each recipient must
justify the choice from its own view. A may already contain the returned denial
text and answer equally well; do not strip that legitimate vendor-visible
information to manufacture B's advantage. B may establish that the proxy denied
a request with particular bytes and reason code, but cannot thereby establish
native vendor-call identity. If the chosen action requires that stronger binding,
leave it unresolved. Native closure/accounting is a secondary evidence-quality
question, not a value win merely because only B contains the proxy's files.
C must justify an additional supported decision about positively retained host
observations to warrant kernel integration. This technical value question does
not establish customer demand or a USP.

After separate authorization, prepare the views and gate each independently.
Use fresh separate recipients without exposure to other views; the previous
packet-only recipient has seen B material and is ineligible for A. If separate recipients are unavailable, use
one fresh recipient sequentially A then B then C, freezing each answer file and its SHA256 first, and
label the outcome a contaminated diagnostic comparison, not a blind study.
Same six questions, instructions and 20-minute inspection ceiling per view;
record actual commands/time/friction and no automatic retries. An exceeded
budget is an unfinished inspection, not a source's negative answer.
One bounded round only. Do not fill in the answer table while defining this contract.

The oracle remains outside all views. Only a separate adjudicator may compare
frozen answers with original fixture wire records afterward. Oracle evidence
covers fixture-received frames, not vendor rejection, all task activity or
execution success. Keep adjudication findings separate from recipient answers;
never retroactively replace an answer or present oracle-assisted work as cold.

## Controls, limits and acceptance gates

Before executing the comparison, pin exact view membership and all reader/source
hashes; apply existing size/depth/occurrence ceilings before materialization.
Keep original design limits (64 incident input occurrences, 256 MiB outer source
bytes, 240 MiB declared occurrence bytes, and all nested limits). They are ceilings, not an
admission claim for these experimental sources. No new production parser or
schema is authorized by this document. Existing receiver limits remain effective.

Each authorized offline control uses a disposable copy; originals stay unchanged:

- Missing source and duplicate occurrence: retain unknown/unavailable or refusal;
  never merge duplicates into a supposedly unique call.
- Foreign-session or valid identity swap: existing binding rules must refuse the
  affected claim; a hash match alone cannot admit it.
- Byte corruption and semantically invalid input with recomputed manifest:
  distinguish integrity detection from canonical semantic refusal. Unsupported
  source semantics are unavailable, not a newly implemented validator.
- Coherent source replacement: an independently pinned expected digest detects
  it; without an external expectation, record that authenticity is not established.
- Apply existing hostile-input and output-preservation tests from the original
  design when a changed parser/projector makes them relevant; do not claim their
  coverage from this document or build another general harness.

The current capture has no demonstrated parallel-identical or reused-ID control;
positive file/process/network coverage still needs kernel verification. The
original design's full acceptance therefore remains **unmet**. A diagnostic
comparison may report bounded findings but cannot upgrade them to full acceptance.
Synthetic controls cannot fill missing real controls. Any further capture needs
new authorization and its own identity; it cannot be spliced into this run.

Stop the affected view on unknown source/version, attribution ambiguity,
ineligible health, missing canonical method for a claimed verification, mismatched
provenance, leaking scope, absent control required for the diagnostic method, or
exceeded resource bound. Only the explicitly unavailable evidence above may be
reported as unavailable without blocking a diagnostic view; it never passes the
corresponding full-acceptance gate. Mark C unavailable
while its gate is unmet; do not substitute fixture/oracle events for kernel bytes.
If A yields equally sufficient supported decisions, stop broader integration or
justify only a portability adapter. If no supported incremental decision changes,
do not productize the comparison. If identity/window conditions remain missing,
stop strong joins. Any new predicate or stronger guarantee needs a separate
compatibility decision/ADR, not a footnote here.

## Review and execution boundary

Claude Code Desktop is a design consultant. A non-building reviewer reviews the
frozen document bytes and their consistency with existing contracts; that review
is not implementation, source-admission or empirical A/B/C proof. Record reviewer
identity separately from any publisher. This document authorizes no execution.
No new model session, projection, comparison, public evidence publication,
push, PR, merge or release follows automatically. **Execution remains HOLD.**
