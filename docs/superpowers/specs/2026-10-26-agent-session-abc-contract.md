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
| A | Original supported vendor events and hook occurrences, documented native inspection route, relevant capture settings | Source attribution, lifecycle reconciliation and byte-preserving packaging not yet established |
| B | A plus original proxy decisions, request receipts, session context and native stop; applicable policy/mode configuration and necessary proxy diagnostics | Proxy subset replay passes; combined A+B view not yet built or tested |
| C | B plus original kernel evidence and its capture/health provenance | Blocked until pinned canonical archive verification and health/coverage eligibility pass |

Each view gets its own exact-membership manifest, externally selected expected
digest, passive readers/instructions and source-specific provenance. Identical
common material contains only scope, questions, bounds and integrity metadata.
It must not disclose richer-view answers or B/C-only file names, counts and hashes to A: no shared supervisor summary containing
B's stop/receipt verdicts, no shared kernel-health conclusions. Retain the full
original supervisor privately; expose only necessary, explicitly labelled
producer declarations to the appropriate view. A missing lifecycle fact is
unavailable, never silently borrowed from B/C. B's existing closeout reader may
receive its required original supervisor; C-only result fields must first be
shown absent, otherwise packaging stops rather than feeding it a rewritten record.

`native.stdout` mixes runner and vendor material. Do not give A that entire stream
or reserialize selected JSON as original bytes. Before packaging, freeze an
explicit source-attribution rule, exact ordered byte ranges, source digest and
projection digest; account for every original byte exactly once (including runner framing) and
prove each range is attributable without inferring from the
expected answer. Preserve the original stream outside recipient views. If this
cannot be established, stop packaging and record the ambiguity. The complete range accounting and mixed-source digest remain with the
adjudicator; each recipient receives only its eligible excerpts, their hashes
and source-local navigation. Derived maps
are navigation metadata, never new source events or identity evidence.

A receives a documented offline native inspection route; unsupported Assay
parsing must not be scored as a vendor-evidence failure. Leak checks must distinguish added B/C-only metadata from genuine vendor content
that happens to mention a proxy or its denial. Do not censor the latter.
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
ineligible health, missing canonical method, mismatched provenance, leaking
scope, absent required control, or exceeded resource bound. Mark C unavailable
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
