# Single-session agent evidence: proposed goal and proof contract

Status: proposed design, not an implemented format or a measured capability.
Date: 2026-10-22. Code baseline: `8b174f8f9e6bb60e1e391dd4cab12118829e3b65`.
Related scope: #1973, #1974, #2490, #1976, #2496; reuse #2493 and #3345.

## 1. Complete outcome

A developer hands a bounded Linux coding-agent session to an investigator who has
neither the developer's checkout nor a vendor account. Using retained original
bytes, a pinned offline verifier, and independently selected verification context,
the investigator can distinguish:

1. what the agent runtime reported;
2. what the configured MCP proxy observed and decided;
3. which positive filesystem, process, and network observations the host recorder retained;
4. which records can be joined under an explicit identity contract;
5. what remains unobservable, ambiguous, missing, or outside the capture boundary.

The deliverable is a usable capture-to-handoff-to-offline-verification workflow,
not merely an importer, another dashboard, or a passing synthetic fixture. It must
show whether adding Assay changes a concrete investigation answer compared with
the best available vendor records of the same task. Market demand and superiority
over other products are hypotheses, not consequences of passing this experiment.

First host candidate: one pinned Claude Code CLI on Linux, headless execution,
one controlled local MCP server, and explicitly exercised built-in tools. The
initial workflow is a short session fitting the existing 64-input budget, including
all hook lifecycle occurrences and other evidence inputs; it does not promise an
arbitrarily long coding session. Only
selected local observation boundaries are covered. This does not cover all agents,
remote MCP execution, all deployment profiles, or every action in a session.

## 2. Existing contracts and architecture choices

Normative constraints remain [ADR-042](../../architecture/ADR-042-evidence-first-positioning.md),
[ADR-043](../../architecture/ADR-043-evidence-chain-integrity-invariants.md), and
[SPEC-Incident-Package-v1](../../architecture/SPEC-Incident-Package-v1.md).
This proposal does not amend their wire formats.

| Option | Benefit | Cost and disposition |
| --- | --- | --- |
| Vendor records alone | Lowest integration cost; native call and session context | Required comparison baseline; choose this if added sources do not improve a predefined investigation answer |
| Vendor records plus existing Assay proxy, Runner and incident package | Reuses capture, bounded verification and offline handoff | Preferred conditional design; new source admission and preparation may be needed, identity propagation is unproven |
| New general collector or instrumented identity bridge | Could introduce new correlation | Rejected as default; bridge needs separately demonstrated demand and a reviewed contract because argument injection can change tool behavior |

Reuse the existing incident exporter and verifier. The exporter currently requires
inventory, recorded assessment and object inputs; it does not discover a session
or construct that assessment. If preparation is necessary, expose a bounded path
that calls shared validation/derivation functions. Do not implement a second
assessment engine in a recipe. Keep the standalone reader's existing fixture
parity obligation; do not introduce an evidence-library dependency into that leaf.

## 3. Vendor comparison, as documented on 2026-10-22

Documentation establishes available interfaces, not successful capture on our host.

| Source | Existing capability relevant to this investigation | Boundary for this design |
| --- | --- | --- |
| [Claude monitoring](https://code.claude.com/docs/en/monitoring-usage) | OTel events and optional traces, tool decisions/results, tool-use correlation | Input logging can truncate values; rejected calls need decision events rather than an expected result event. A vendor ID does not establish its propagation across MCP |
| [Claude hooks](https://code.claude.com/docs/en/hooks) | Per-invocation input with tool identity and inputs; lifecycle-specific payloads | Preferred source candidate. Preserve stdin bytes separately. Hooks are vendor/host-reported, not an independent authority. Admit only the lifecycle shapes actually pinned and captured |
| [Claude headless output](https://code.claude.com/docs/en/headless) | Supported structured CLI output, including a streaming mode | Alternative if hook capture is unsuitable; do not support both formats by default. Native internal session files are not the initial dependency |
| [Codex advanced configuration](https://learn.chatgpt.com/docs/config-file/config-advanced) | Opt-in telemetry including API activity, tool decisions and results | Relevant capability comparator; no empirical Codex comparison or absence-of-correlation claim is made here |
| Assay proxy and Runner | Proxy-local decisions; separate host observations and health | Different observation points, not automatically independent trust domains or proof of provider outcomes |

The preferred capture source is one original JSON stdin payload per hook invocation.
A recorder copies bounded bytes without adding an envelope or reserializing JSON.
If the actual interface differs, decide admission after inspecting retained bytes;
do not invent an NDJSON stream by rewriting records. Collector-serialized OTel
exports are labeled as such, not as original vendor wire bytes. Pin vendor version,
telemetry settings, hook configuration, collector version/configuration if used,
and the actual executed binaries. No source token is frozen before the probe.

## 4. Evidence ownership and chain

| Artifact or fact | Author / observation point | Validation and allowed inference |
| --- | --- | --- |
| Vendor hook payload | Runtime, captured by a local recorder | Strict bounded source validation; proves retained producer report, not execution truth |
| Proxy decision | Assay proxy at the configured MCP boundary | Existing source validator; distinguishes decision from enforcement and outcome |
| Startup context | Proxy startup producer | Currently not admitted by incident v1; any addition is context-only with an explicit association rule, never executable identity or per-call proof |
| Kernel records and health | Runner and its host | Existing admitted format and retained health; positive host observation, current coverage ceiling remains `limited` |
| Inventory and recorded assessment | Package preparer | Existing incident validator recomputes allowed results; preparer assertions are not independent evidence |
| Expected digests, keys, scope and budgets | Receiver-selected context | External expectations constrain the package; author-supplied context alone does not authenticate it |
| Control oracle | Owned test server and controlled workload | Separate test evidence, withheld from recipient views; scores answers, never supplies missing package evidence |

Hash exact retained bytes before semantic parsing, with streaming size limits.
Preserve input occurrences even when object bytes deduplicate. Each reference must
resolve to a digest-bound occurrence and format-defined byte/record locator. A new
JSON-object format needs its own locator rule; existing NDJSON locators must not
be applied to a fabricated stream. Capture errors and truncation remain retained
failures. Never silently drop a record to fit the package.

Record source commit, executed binary version and digest, Linux/kernel identity,
command/configuration, negotiated MCP version, capture scope and interval,
recorder health, artifact hashes, and exact replay commands. Distinguish operator
declarations from observed fields. A filename, adjacent directory, timestamp or
argv digest does not prove session membership. Secrets require a private original
handoff or fresh synthetic fixtures; redacted bytes are derivatives, not originals.

## 5. Claims, joins and compatibility

Preserve existing result vocabularies and their scope:

- Surface observation: `unknown`, `limited`, `adequate`.
- Join: `confirmed`, `contradicted`, `unobservable`.
- Claim decision: `supported`, `degraded`, `blocked`.
- Verification: `package_verified`, `package_refused`, `verification_unavailable`.

A verified package is not a verified whole session. Source digests do not establish
source authenticity. Same-host recorders are not independent security authorities.
No scalar trust score, safe-agent claim, compliance claim, generic identity layer,
or provider-outcome verification is introduced.

Current `strong_key_value` recognizes specific source paths. Proxy fallback IDs
prefixed `req_` and `gen_` are not strong. Vendor `tool_use_id` is not automatically
an MCP request ID or the current `arguments._meta.tool_call_id`. Time, order,
matching tool names/arguments and session IDs do not repair missing strong identity.

Before enabling a new strong join, the real probe must establish the source field,
its producer, propagation into the admitted counterpart, scope, uniqueness,
retry/reuse behavior, and a trustworthy window representation under the existing
contract. Hook payloads must not be assumed to contain timestamps. Recorder wall
clock is a declaration, not a native source timestamp. Pre/post records often
repeat an ID: specify lifecycle populations rather than erasing duplicates to
manufacture uniqueness. If these requirements cannot be met, retain diagnostic
context and report `unobservable`; do not silently extend window semantics.
If native timing is absent, the preregistered alternatives are diagnostic-only
handoff or an explicit source-choice revision to a documented timestamped vendor
source. Neither alternative is assumed to satisfy strong binding.

The current `equal_retained_value` implementation uses the compared pointers for
strong-key eligibility too. It does not first bind identity and then compare
arbitrary arguments or outcomes. General semantic disagreement is therefore not
promised by this slice. If needed later, specify a separate predicate/identity
contract and version it explicitly instead of reinterpreting v1.

`counterpart_present` can support a scoped negative only with the existing strong
binding and adequate, complete applicable surface/window evidence. A PreToolUse
notification, PostToolUseFailure notification, rejected request, built-in tool,
or unwrapped server does not by
itself entail a proxy counterpart. Kernel observations never establish per-tool
causality or an exhaustive negative under the current admitted source contract.
Startup context, if admitted, must not upgrade observation or enforcement claims
merely because its fields look plausible.

A new source token can follow the existing additive incident-v1 admission
precedent only if old semantics and field shapes remain unchanged. Amend closed
admission tables, validators, schema/fixture parity, and minimum-reader guidance
together. Older readers refusing the new token is expected, not interoperability
success. Changes to existing predicates, public Rust structures, limits, or CLI
contracts need separate compatibility assessment and, where necessary, a new
version/ADR. Breaking changes are allowed when justified, not preselected.

## 6. Preregistered comparison and acceptance

Use three nested views of the same retained task: A vendor only; B vendor plus
proxy; C vendor plus proxy plus kernel/health. Give every view the same questions,
receiver scope and integrity checks. Do not make A fail by using a verifier that
cannot read the vendor format. Provide the documented native inspection route
and retained telemetry settings as part of the baseline. The oracle stays outside
all three views. Source membership differs; expected digests reflect each view.

Questions and admissible answers:

| Question | Required evidence | Important limit |
| --- | --- | --- |
| What tool/input did the runtime report? | Original supported vendor record and exact reference | A report is not an execution outcome; truncated telemetry cannot prove exact arguments |
| What decision did this proxy record? | Proxy record and applicable mode/policy context | An allow is not successful execution; unknown mode is not enforcement |
| Is the vendor call the same as this proxy call? | Proven native binding plus admitted uniqueness/window rules | Otherwise unobservable, including identical concurrent calls |
| Which host operations were retained? | Positive kernel record with provenance and health | No per-call causal attribution or absence proof |
| Does a specific proposition contradict retained evidence? | Existing predicate, eligible binding and required coverage | Missing records alone are not contradiction |
| Can a cold receiver reproduce the answer? | Package, pinned passive verifier and independently selected context | No vendor login, original workspace, live model or network during verification |

The initial controlled task must include: allowed MCP call, proxy denial,
vendor-side rejection, built-in tool, parallel identical calls, retry/reused ID,
and positive file/process/network activity in the declared local scope. A task
that fails to exercise a required control is incomplete, not passing. Replays
also cover a blind/missing source, duplicate invocation, foreign-session input,
malformed source with correctly recomputed digest, corrupted package, and valid
source replacement. For a coherently replaced package, test both cases: externally
pinned digest detects the swap; without external pinning do not promise detection.

Add boundary controls for unknown source/version, JSON duplicates/depth/size,
occurrence surplus, limit and limit+1, and output failure preserving old bytes.
New production-path tests must fail under validator bypass, eligible-key widening,
coverage promotion, stale producer identity, or stale generated output. Reuse
existing incident matrix coverage rather than rebuilding all tests in another harness.

For every question, record A/B/C answer, exact references, declared limits,
oracle comparison, unresolved ambiguity and recipient friction. At least one
predeclared investigation decision must improve due to retained proxy/host evidence
without unsupported conclusions for productization to proceed. An explicit,
correctly bounded unknown can be useful, but count it only when the receiver can
name the decision it changes. Deterministic synthetic controls demonstrate contract
behavior; they do not substitute for the real Linux task or an unaided receiver.

Cold verification runs on a clean Linux machine with network disabled and no
original checkout. Record verifier digest, input hashes, command, exit code and
fresh results. Repeat with damaged bytes. Independent review checks the exact
final head and evidence; the co-designer cannot supply the non-building quorum.
A user-demand decision remains separate from this engineering acceptance.

## 7. Resource bounds and stop criteria

Existing hard limits apply before materialization: incident input occurrences 64,
outer source bytes 256 MiB, declared occurrence bytes 240 MiB, and all nested
source/entry/depth limits in incident §8. Do not add limits or raise them for this
probe. Select a task whose lifecycle records fit after reserving inputs for other
sources. Per-invocation hook files deliberately trade capacity for exact bytes.
Declare a 30-minute maximum capture window and stop on the first exceeded budget;
this is an experiment budget, not a product performance promise. Preserve failure
bytes and report unavailable/incomplete as appropriate.

| Stop condition | Required disposition |
| --- | --- |
| Vendor-only answers the same predefined decisions with equally sufficient retained evidence | Stop broader integration, or justify only a small portability adapter |
| No stable native identity/window/lifecycle basis | Stop strong-join work; retain diagnostic findings. Any instrumented bridge requires a separate design and task-semantics tests |
| Source shape/version cannot be pinned or recorder changes inputs | Do not ship source admission; try the documented alternative only through an explicit source-choice revision |
| Capture privileges, kernel health or MCP negotiation are unavailable | Mark unavailable/limited; do not weaken security, substitute mock data, or call it live proof |
| New behavior requires v1 predicate reinterpretation | Stop that slice; decide a new contract/version before coding it |
| No correct incremental receiver decision, or recipient still needs author explanation | Do not claim the full outcome; fix a demonstrated handoff gap or stop productization |
| Resource/privacy boundary exceeded | Stop capture, retain failure provenance privately; no silent dropping, rewriting, or limit increase |
| Implementation grows into multi-host telemetry, general identity or remote attestation | Re-scope explicitly; these are not dependencies of this outcome |

Measure preparation effort, package size, verifier time/peak memory and receiver
friction with provenance, without a broad benchmark project. Nested evidence views
do not measure runtime overhead: any overhead claim requires separately declared
paired executions. Existing independent performance investigations remain separate.

## 8. PR division and decision dependencies

These are proposed PR boundaries, not opened PRs or implementation authorization.

| PR | Deliverable | Gate / scope boundary |
| --- | --- | --- |
| 1 — contract and Linux probe | This public-safe design, bounded capture recipe, compact retained control evidence and A/B/C decision table | Pin source shape and negotiated protocol; decide identity, timestamp/lifecycle feasibility and incremental value. No claimed importer or strong join |
| 2 — bounded vendor admission | One measured source format, original-byte locators, strict validation, standalone parity and discriminating negative tests | Only after PR1 establishes a useful source. Preserve old outputs/semantics; document older-reader refusal |
| 3 — preparation and justified binding | Minimal shared preparation/export path; source association and strong joins only where proven; context admission only if needed | No duplicate assessment engine. If binding is unavailable, explicitly decide whether a diagnostic-only handoff still meets user value before proceeding |
| 4 — cold recipient and supported recipe | Fresh Linux recipient trial within the declared 64-input budget, damaged package, provenance, friction fixes and single-host instructions | Demonstrate complete handoff using final code; independent exact-head review and required CI |

Keep contract changes and their tests together. Do not split producer/validator
changes into independently releasable incompatible states. If PR2 and a small
PR3 are inseparable, combine them; if a versioned predicate is needed, introduce
a separately reviewed design rather than hiding it in preparation. Stop after
PR1 if the evidence does not justify the remaining work.

#1974/#2490 receive one measured host decision, not automatic closure of wider
adapter/demand scope. #1976/#2496 receive one workflow, not completion of three
deployment profiles. #1973 closes only against its full refreshed acceptance
criteria. No issue state is changed by this document.

## 9. Current evidence status

Code and documentation analysis plus Claude Code Desktop co-design only. No new
Linux capture, compatibility test, cold-receiver trial or market validation has
run for this proposal. The source pin, native binding feasibility, lifecycle/window
representation and incremental receiver value are explicit PR1 decision gates.
