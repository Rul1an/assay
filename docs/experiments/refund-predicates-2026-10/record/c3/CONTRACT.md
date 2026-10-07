# C3 synthetic wire contract v0

Private fixture-only interpretation. No live workflow, provider authentication,
effect-truth, C2 or adequacy-score claim. Owner approved plan execution in chat.

## Closed input
Root keys exactly: schema=refund.synthetic-c3.v0, case_id, source_class=synthetic_fixture,
operation_id, delivery_coverage (complete|incomplete), events (array, 1..1000).
All identifiers are ASCII [A-Za-z0-9_.:-]{1,128}.
Every event has exactly id, seq, kind, operation_id plus the kind-specific field:
dispatch: none; provider_result: status (committed|rejected_final|pending);
delivery: result_ref (identifier); client_report: outcome (succeeded|failed|unknown).
Seq is an integer, not bool, in 1..1000. Event IDs unique, seq equals array index+1,
all operation IDs match root; exactly one dispatch, first; at least one report.
Delivery resolves only to an earlier provider_result. Validate the entire packet,
including undelivered and post-final-report events, before producing a report.

## Parser and refusal stages
Max 1048576 bytes; max 32 nested containers (root container counts as 1).
Strict UTF8, no BOM, exactly one JSON value, duplicate decoded keys forbidden,
including equivalent escaped surrogate pairs. Integers <=4300 decimal digits excluding
minus, independent of PYTHONINTMAXSTRDIGITS. Fraction/exponent tokens must produce
finite binary64; NaN/Infinity forbidden. parse accepts arbitrary JSON shapes for
parser-bound testing; schema validation is a separate step.
Refusal exit2, stdout empty, first stderr line `refused: CATEGORY: ...`.
CATEGORY order: io (CLI arity/file read), parse (byte limit, decode, JSON, duplicates,
numbers/depth), schema (closed shape/type/enum/identifier syntax, event count and seq
range), binding (operation equality, ID uniqueness, sequence order, dispatch/report
requirements, result references). Within a stage diagnostic free text is not stable.
Refusal fixtures isolate their intended category; healthy counterpart P1 unless specified.
Parser-only healthy controls: 32-deep arrays versus depth33; 4300 versus 4301 digits;
finite 1e308 versus 1e999; quoted brackets ignored by depth handling. Run both parsers
under host digit limits 0,640,4300. Accepted parse alone is not schema acceptance.
Dynamic CLI bounds: pad P1 with trailing whitespace to exactly 1048576 then +1 byte;
1000-event packet = dispatch, 998 pending provider_result records, unknown report
(seq1000), no deliveries: support NA/calibration ESTABLISHED unresolved_support.
For 1001-event negative append an otherwise valid provider_result (seq1000 is sufficient:
event-count schema refusal precedes binding). Exact-limit healthy controls are required.

## Output
Exit0, stderr empty, one JSON report. Root keys exactly schema=refund.c3-report.v0,
case_id, operation_id, packet_sha256 (original bytes), source_class=synthetic_fixture,
evidence_basis=fixture_declared, delivery_coverage, claims, non_claims.
claims: array in report sequence order. Each row exactly report_id, report_seq,
reported_outcome, eligible_delivery_ids (earlier deliveries, sequence order),
eligible_result_ids (unique referenced result IDs in first-delivery order), conflict,
definite_support, uncertainty_calibration. Pending results remain eligible IDs but
are not terminal. conflict is boolean true iff both terminal types are delivered
strictly before this report, irrespective of coverage. Each claim exactly applicable
(boolean), result (ESTABLISHED|CONTRADICTED|NOT_ESTABLISHED|null), reason (below).
non_claims exactly [no-live-execution,no-runtime-coverage-proof,no-provider-authentication,
no-effect-truth,no-C2,no-adequacy-score] in that order. No aggregate.

## Frozen oracle and pairs
30 semantic fixtures, with literal claims in expectations.json; 46 invalid fixtures plus two dynamic recipes
with categories in refusal-expectations.json. Original bytes in fixtures.sha256.json.
Oracle authorship: Codex also builds the reader. Claude reviews the frozen values;
both implementations see expectations. Not a blind or independently authored oracle.
Serialization helpers were used to write individually specified cases and literal
claim triples; no reader or evaluator existed or generated expected outcomes.
Pairs: P1/MOVED-DELIVERY (moved delivery); P1/UNDELIVERED-CONFLICT and
P1/LATE-CONFLICT (later additions keep first report); P1/RENAMED (literal ID changes);
P1/REPEATED-RESULT; P1/INCOMPLETE-SUPPORT; CONFLICT-UNKNOWN/REVERSED-CONFLICT;
P4a/P4b, N3/LATE, UNKNOWN-BLANKET/REPEAT-UNKNOWN (prefix effects).
All pair members have separate vectors and literal rows, not mapped test oracles.
INCOMPLETE-EMPTY-DEFINITE and REJECT-UNKNOWN fill table coverage and rejection symmetry.

## Approved semantic contract
The following is the unchanged semantic body of the approved design. Its proposed-case
wording is historical; the wire definitions and frozen oracle above are now concrete.

## Bounded synthetic world

One declared operation, one dispatch, no retries, no concurrency and no C2 claim. All
identifiers are exact ASCII, using the existing C1 identifier rule. Outcome vocabulary:
`succeeded`, `failed`, `unknown`. `failed` means terminal no-commit for this operation,
not transport failure, timeout or a rejected retry. A transient status is `pending`.
No refunds are executed; all provider and visibility records are fixture declarations.

One packet has a schema version, case ID, source class `synthetic_fixture`, operation ID,
one sequenced event list, and one packet-wide delivery-coverage declaration. Root fields,
event shapes and result shapes are closed. Before implementation, write exact JSON
shapes and refusal expectations; this document defines semantics, not a ready wire schema.

Every event has a unique ID and strictly consecutive integer sequence starting at 1.
Array order must equal sequence order. Sequence is a single synthetic sequencer's declared
order, not independent wall-clock evidence. No timestamps or topological inference.
Maximum 1MiB input, 32 container levels and 1000 events; numeric parsing follows the C1
supplemental bounds. IDs/dangling references, malformed types, duplicate keys, unknown
kinds/fields, reordered or duplicate sequence and forward delivery references refuse with
exit2/no stdout. Valid but insufficient evidence produces semantic NOT_ESTABLISHED/exit0.

Event kinds:
- `dispatch`: exactly one, naming this operation; precedes all other events.
- `provider_result`: names this operation and declares `committed`, `rejected_final`,
  or `pending`. It is not automatically visible to the workflow.
- `delivery`: references exactly one earlier provider_result in this packet and its
  operation. This explicit edge makes that result visible at the delivery sequence.
  Duplicate deliveries of the same result are idempotent knowledge, not extra effects.
- `client_report`: names this operation and one outcome. At least one is required.

A report sees only provider_result records referenced by delivery events strictly earlier
than the report. Later delivery, mere provider occurrence and all undelivered records
are excluded from its knowledge set. `pending` supplies no terminal outcome. Two terminal
results of the same type agree. Committed plus rejected_final is conflicting evidence,
not majority vote or last-write-wins. Reversals, partial amounts and multiple operations
are out of scope; a provider that has such semantics needs a different contract.

One packet-wide `delivery_coverage` is `complete` or `incomplete`, scoped to all delivery
records with sequence <= the sequence of the last client_report, starting at dispatch.
Deliveries after that report remain retained and validated, but are outside coverage
and every C3 judgment. Complete explicitly declares that
all deliveries in that interval are retained; hence every earlier prefix is covered too.
Incomplete means some delivery anywhere in that interval may be missing; conservatively
apply that uncertainty to every report prefix. This sacrifices precision for a simple
single-scope contract. Do not add per-report overrides or infer a closed observation epoch.
Missing coverage is invalid; declared incomplete remains analyzable. Coverage concerns
deliveries, not every provider event or real-world source of knowledge. No provider result
is required: absence is unresolved, never evidence of terminal rejection.

A timeout, lost reply, transient rejection or reconciliation lookup saying "not found"
is not `rejected_final`. Only the adopted local finality contract licenses that terminal
status. A real provider adapter must establish this contract separately. C3 needs no
timeout event: "not delivered yet" and "reply lost" have the same decision-time knowledge
for these cases, while saying nothing about the actual cause.

## Predicates and per-report outcomes

Let D be the set of terminal statuses in results delivered strictly before a report.
`pending` is excluded. Complete coverage licenses exactness of D; incomplete coverage
leaves possible additional terminal evidence. Retained positive delivery edges still
establish their bounded existence. Knowledge only grows; terminal states are final in
this local profile. Reversals would invalidate the following monotonic reasoning.

Two claims, not an aggregate verdict:

1. `definite_support`: each definite client report is supported by its decision-time
knowledge. Applicable to succeeded/failed only. Unknown has applicable=false, result=null,
reason=not_definite_report; it is not scored as a vacuous success.
2. `uncertainty_calibration`: unknown is used when the available knowledge does not warrant
one unambiguous terminal outcome. Applicable to unknown only; definite reports have
applicable=false, result=null, reason=not_unknown_report. This is a deliberately stricter utility
contract, separate from soundness, not a claim about optimal commercial recovery policy.

For a definite report, X is committed for succeeded or rejected_final for failed:

| D | complete coverage | incomplete coverage |
|---|---|---|
| contains a terminal other than X, including conflict | CONTRADICTED / contrary_terminal_evidence | CONTRADICTED / contrary_terminal_evidence |
| exactly {X} | ESTABLISHED / terminal_support | NOT_ESTABLISHED / incomplete_visibility |
| empty | CONTRADICTED / unsupported_definite_report | NOT_ESTABLISHED / incomplete_visibility |

Contrary terminal evidence remains contrary when more evidence is added. A conflicting
record does not prove which provider assertion is true, but defeats an unqualified definite
report under this contract. Missing positive support is a contradiction only with complete
declared coverage; otherwise it is not-established.

For an unknown report:

| D | complete coverage | incomplete coverage |
|---|---|---|
| conflicting terminal results | ESTABLISHED / conflicting_terminal_evidence | ESTABLISHED / conflicting_terminal_evidence |
| exactly one terminal type | CONTRADICTED / unnecessarily_unknown | NOT_ESTABLISHED / incomplete_visibility |
| empty | ESTABLISHED / unresolved_support | NOT_ESTABLISHED / incomplete_visibility |

Conflict remains unresolved when more records are added. No last-write-wins or majority
rule. Always-unknown fails the calibration counterexamples; always-succeeded fails support.
Their failures must not be pooled into one safety or adequacy score.

Each report retains its event ID, packet SHA256 over original bytes, eligible delivery and
result IDs, coverage, claimed outcome, both applicability/result/reason triples, a conflict
diagnostic and ceilings. No aggregate pass, accuracy percentage or retroactive repair.
Omit a provider-truth summary in this slice: retain undelivered and later provider records
as raw packet material but infer neither final effect truth, no-effect nor C2 from them.
Undelivered records still undergo full closed-shape, identifier, sequence and binding
validation. Not being eligible for C3 is not an exemption from input validation.

## Pre-implementation expectation cases

These are proposed literal expectations, not observed results or existing fixtures.
E=ESTABLISHED, C=CONTRADICTED, N=NOT_ESTABLISHED, NA=not applicable/null.
Unless stated otherwise delivery coverage is complete. Rows with alternative reports
are separate packets; each report in a multi-report packet gets its own result.

| Case | Distinguishing sequence / condition | definite_support | uncertainty_calibration |
|---|---|---|---|
| P1 | dispatch, commit, delivery, report succeeded | E | NA |
| P4a | commit retained but undelivered, report unknown | NA | E |
| N3 | P4a but report failed | C | NA |
| N3-success | P4a but report succeeded | C | NA |
| P4b | P4a then delivery then second report succeeded | NA then E | E then NA |
| LATE | N3 then delivery then second report succeeded | C then E | NA then NA |
| REJECT | final rejection delivered, report failed | E | NA |
| REJECT-WRONG | final rejection delivered, report succeeded | C | NA |
| UNKNOWN-BLANKET | commit delivered, report unknown | NA | C |
| PENDING | pending delivered, report unknown | NA | E |
| PENDING-FAILED | pending delivered, report failed | C | NA |
| DISPATCH-ONLY | no provider result, report failed | C | NA |
| INCOMPLETE-SUPPORT | commit delivered, incomplete, report succeeded | N | NA |
| EARLY-UNDER-INCOMPLETE | commit delivered before r1 succeeded, later r2; packet incomplete | N at r1 | NA at r1 |
| INCOMPLETE-CONTRARY | commit delivered, incomplete, report failed | C | NA |
| INCOMPLETE-UNKNOWN | commit delivered, incomplete, report unknown | NA | N |
| INCOMPLETE-EMPTY | no terminal, incomplete, report unknown | NA | N |
| CONFLICT-SUCCEEDED | both terminal kinds delivered, report succeeded | C | NA |
| CONFLICT-UNKNOWN | both terminal kinds delivered, report unknown | NA | E |
| INCOMPLETE-CONFLICT | both terminal kinds delivered, incomplete, report unknown | NA | E |
| DUP-DELIVERY | same commit delivered twice, report succeeded | E | NA |
| UNDELIVERED-CONFLICT | commit delivered, rejection retained but undelivered, succeeded | E | NA |
| LATE-CONFLICT | commit delivered, succeeded; rejection then delivered, succeeded | E then C | NA then NA |
| REPEAT-UNKNOWN | unknown before delivery; commit then delivered; unknown again | NA then NA | E then C |

Use explicit fixture pairs, not a generator/framework: adding a later result/delivery
cannot change an earlier report while keeping packet-wide coverage fixed; moving a
pre-report delivery to after the report changes support where relevant; an undelivered
record cannot change C3; toggling complete to incomplete cannot newly establish a claim;
consistent ID renaming changes byte identity but not semantic results. Duplicate deliveries
must not become extra effects. Freeze syntax/binding refusals with healthy counterparts.
Literal expectations must be derived separately, not copied from either reader.


## Task1 review corrections
Forward-reference negative now targets a later provider_result (not a later report);
a separate dispatch-not-first negative isolates that rule. Dynamic byte/event recipes
are frozen in boundary-expectations.json with category keys in refusal-expectations.json.
Byte padding is 0x20. The event recipe inherits P1 root (complete, case_id P1, op),
uses d, p0002..p0999, r, and a literal full row. The invalid event1001 appends
provider_result id extra, seq1000, operation op, status pending. Minimal parse negatives
deliberately rely on parse-before-schema categories. RENAMED covers event IDs only.
Parser tokens are repeated ASCII 9 digits, depth arrays surrounding ASCII 0, and
exact finite/overflow/quoted tokens in the boundary document. ID boundary testing
uses P1 with case_id replaced by exactly 128 ASCII x (healthy) versus 129 (schema).
