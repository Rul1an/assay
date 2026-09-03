# ADR-048: The claim gate shares one lattice and one invariant across three tables that legitimately differ — the two enums move to `assay-common`, the tables and the fold stay home, and the policy/trace path already makes absence claims it cannot base

- Status: Accepted
- Date: 2026-09-02 (revised after seven review passes through 2026-09-03;
  §"What the reviews corrected" lists every change)
- Supersedes: none
- Amends: none. ADR-046 governs when two vocabularies stay separate; §"Why ADR-046 does not decide this" states where its test lands the other way and where it does not.

## Context

`crates/assay-evidence/tests/claim_gate_parity.rs:9-14` has carried an open architectural question
in its own header since it was written:

> Ideally one would call the other. It cannot today: the runner substrate is documented as internal
> and API-unstable, and its inputs are runner-domain vocabulary that an evidence pack does not have.
> Promoting the shared mechanism into `assay-common` is a real ADR question — the CLAUDE.md
> admission test ("a mechanism whose second implementation would silently mean something different")
> is arguably met, and this file is the evidence that it is met, since the first draft of the
> evidence-side rule *did* silently mean something different.

`crates/assay-cli/src/cli/commands/evidence/verify_side_effects.rs:162-164` says the same thing
from the CLI side: the `SideEffectLevel → CodingAgentSourceClass` mapping is placed in the CLI on
purpose because *"`assay-mcp-server` does not depend on `assay-evidence` and must not start to for
this... a new crate edge stays an ADR question rather than a side effect of wiring."* This is that
ADR.

The prompt to take it now is external: OWASP ACS
[#16](https://github.com/GenAI-Security-Project/agent-control-standard/issues/16) is specifying a
locally-derived `coverage_status`, and two participants agreed on 2026-09-02 that observation,
corroboration and enforcement assurance must not collapse into a single claim. Measured against
that, our tree has the gate on the runner/evidence path — `CoverageDescriptor::claim_decision` on
the runner side and `coding_agent_claim_decision` on the evidence side, pinned to each other by
`crates/assay-evidence/tests/claim_gate_parity.rs`. The third table, `RunnerClaimGate::for_verdict`,
is pinned separately and only within its own crate, by
`crates/assay-runner-schema/tests/claim_support_parity.rs` against the `claim_support` projection;
no test pins it to the evidence gate (§"The parity tests pin less than their names suggest"). The
policy/trace path has no gate at all: its `TraceRecord` carries no capture method (§"The
policy/trace path" below). That asymmetry is what needs a decision.

Measured at `125b036c5` (origin/main), `cargo 1.96.0` / `rustc 1.96.0`. An earlier pass measured
at `056332719` on a feature branch; every cited file is byte-identical between the two. The second
review pass re-measured on this branch at `8481b16be`, whose tree differs from `125b036c5` only in
this file. Before publication, the source claims were revalidated against `4d6d56774` (origin/main,
2026-09-03): every cited source, test, manifest and workflow file remains byte-identical to
`125b036c5`; the intervening changes are confined to release/editor documentation and CI surfaces.

### What is genuinely shared: two enums, spelled twice

| Concept | `assay-runner-schema` | `assay-evidence` |
|---|---|---|
| decision lattice | `ClaimGateDecision{Allowed, Degraded, Blocked}` (`fidelity.rs:32-38`) | `CodingAgentGateDecision{Allowed, Degraded, Blocked}` (`coding_agent.rs:132-138`) |
| claim class | `CoverageClaimKind{PositiveExistence, ExhaustiveSet, BoundedNegative}` (`coverage.rs:26-32`) | `CodingAgentClaimKind{PositiveExistence, ExhaustiveSet, BoundedNegative}` (`coding_agent.rs:120-129`) |

Same members, same order, same `#[serde(rename_all = "snake_case")]`, same derive set. Neither is
`#[non_exhaustive]`. That is one answer typed twice.

A **third** spelling of the neighbouring vocabulary lives in
`crates/gateway-evidence-replay/src/schema.rs:11-38` (`Coverage`, `SourceClass`, `Ceiling`). It is
a leaf crate with no internal dependencies by design — a standalone verifier that must not import
the thing it verifies — so it is deliberately **not** a target of this ADR.

### What is shared beneath the enums: one invariant, not one table

Absence is never more permissive than occurrence. It is pinned by
`crates/assay-runner-schema/tests/claim_support_parity.rs:73-85`
(`absence_is_never_more_permissive_than_occurrence`, over every verdict) and it is the property all
three tables below hold.

### What is NOT shared: the decision tables differ in their inputs, and one of them in a reading

The first draft of this ADR called the three sites "one rule over three vocabularies". Measured,
they are three tables — and the second draft misread two of their cells, which the exact-head
review caught:

| Site | Input | `PositiveExistence` | `ExhaustiveSet` | `BoundedNegative` |
|---|---|---|---|---|
| `fidelity.rs:58-63` `for_verdict` | `Clipped` | **`Degraded`** (as `measured_positive_claims`) | not modelled (`claim_parity.rs:127-130` → `NotModelled`) | `Blocked` |
| `coverage.rs:173-219` `claim_decision_for` | partial descriptor | **`Allowed`** (`:174-182`) | **`Degraded`** (`:191-200`); `Allowed` only under `supports_complete_claims` (`:284-286`: `Full` **and** no blind spots) | `Blocked` (`:209-218`); `Allowed` only under `supports_complete_claims` (`:201-208`) |
| `coding_agent.rs:252-270` `coding_agent_claim_decision` | `Partial` | **`Allowed`** (`:254-259`) | **`Degraded`** (`:260-265`, `partial_coverage_degrades_exhaustive_claim`) | `Blocked` (`:266-268`) |

The lower two rows agree cell for cell on this input, and that agreement is exactly what the
partial leg of `claim_gate_parity.rs` pins. What separates the three tables is the first row
against the other two — `Degraded` versus `Allowed` on the same positive claim, and a claim kind the
fidelity table does not model at all — together with inputs that have no counterpart across the
sites. The runner side has two claim classes with no `ClaimKind` counterpart at all —
`reported_claims` and `per_binding_claims` (`fidelity.rs:41-46`). The evidence side has
`SelfReported`, with no runner analogue. `Degraded` carries four readings: dropped records
(fidelity), the producer's own uncorroborated account (`coding_agent.rs:239`,
`self_reported_degrades_positive_claim`), partial coverage under an exhaustive claim
(`coverage.rs:191-200`, `coding_agent.rs:260-265`), and an effect class the descriptor does not
observe (`coverage.rs:257-266`, `claim_decision_for_effect`). And
`crates/assay-runner-schema/src/claim_parity.rs:3-7` already says it:

> Two questions look like one and are not. `RunnerClaimGate` answers *given how healthy this run's
> observation was, which claim kinds may I make*. A separate rule ... has to answer *given this
> observer class and its declared probe set, which claim kinds can it support at all*. The first is
> the **health half**, the second the **class half**.

So the shared thing is the lattice and the invariant. The tables are domain readings, and CLAUDE.md
(`:161-163`) already fixes what that means: *"What travels is the construction, never the domain's
reading of it."*

### The parity tests pin less than their names suggest

- `claim_gate_parity.rs` never compares against `for_verdict`; the fidelity↔evidence pair is
  **unpinned**.
- Its "complete coverage" leg is dead: no shipped descriptor constructor satisfies
  `supports_complete_claims`, so the test prints *no shipped descriptor reports complete coverage;
  leg skipped deliberately* (`claim_gate_parity.rs:127`) and skips. Live legs are
  `{Absent, Unavailable} × 3` against no descriptor and `Partial × 3` against
  `filesystem_open_syscall_only`, source class held fixed.
- `claim_support_parity.rs` lives in `assay-runner-schema` and pins `claim_support` against
  `for_verdict` **in the same crate**. It is not a two-implementation test and does not depend on
  where the lattice lives.

### Every site that writes a lattice value

| Site | Role |
|---|---|
| `fidelity.rs:50` `RunnerClaimGate::for_verdict` | table |
| `fidelity.rs:142` `projection_claim_level_decision` | table (`Failed → Blocked`, `inconclusive → Allowed`, unknown → `Blocked`) |
| `coverage.rs:149` `CoverageDescriptor::claim_decision_for` | table |
| `coverage.rs:237` `CoverageDescriptor::claim_decision_for_effect` | table |
| `coding_agent.rs:201` `coding_agent_claim_decision` | table |
| `fidelity.rs:119-126` | **override**: `per_binding_claims = Blocked` written after `for_verdict` |
| `verify_side_effects.rs:432-438` | **override**: refutation forces `occurrence_claim = Blocked` and drops the rung |
| `claim_parity.rs:114` `claim_support` | derives from `for_verdict` |
| `coverage.rs:144` `claim_decision`; `coding_agent.rs:463` `coverage_report`; `verify_side_effects.rs:190` `claim_decision_for` | delegate |
| `verify_side_effects.rs:365-366, 369` | **initializer**: three literal `Blocked` defaults in the `CallRow` constructor, each overwritten unconditionally before the row is read (`:420`, `:442`, `:448`) |

Five tables, two overrides, four delegators, three initializers. The overrides matter: they are
lattice literals written outside any table, and a shared enum does not make them one rule. The
initializers do not decide anything — a default that is always replaced is not a decision — but they
are lattice literals, and an inventory headed "every site" has to list them or stop saying "every".

### The fold folds the ceiling ladder, not the lattice

`coding_agent_weakest_ceiling` (`coding_agent.rs:340-360`) short-circuits to `Blocked` on any
blocked decision and otherwise takes `min` over `CodingAgentClaimCeiling` (`:102-110`) — the
five-rung `Asserted..IndependentlyConfirmed` ladder derived from `CodingAgentSourceClass`. It treats
`Allowed` and `Degraded` identically. Its result type distinguishes `NothingClaimed` from `Blocked`
from `Rung` (`:299-309`), and its input carries `rule: String` (`:150`). It is a source-class fold.

The only ordering on the lattice in the tree is `permissiveness` at `claim_parity.rs:170-182`, a
free function whose doc refuses to be an `Ord`: *"an ordering on the public enum invites arithmetic
on it, and the only thing this crate needs to say is that absence is never looser than occurrence."*

### The policy/trace path: what is absent, what is unplumbed, and what it already claims

`assay-core/src/coverage_next/types.rs:97-101`:

```rust
pub struct TraceRecord {
    pub trace_id: String,
    pub tools_called: Vec<String>,
    pub rules_triggered: HashSet<String>,
}
```

No capture **method** reaches this struct, and `assay-core` has no dependency on
`assay-runner-schema` (`cargo tree -p assay-core -e normal,dev,build`). But the first draft's
"absent, not merely unplumbed" was too strong, and in the wrong direction:

- **Per-record completeness exists, is unplumbed, and sits on a different wire shape.** The V2
  trace schema carries `TruncationMeta{field, original_len, kept_len, sha256, strategy}` on
  `StepEntry.truncations` and `ToolCallEntry.truncations` (`assay-core/src/trace/schema.rs:52-58,
  74, 97`), attached to a record tagged `type: "tool_call"` whose arguments live in `args`
  (`:31, 87`). The reader on this path, `assay-cli/.../coverage/legacy.rs`, takes raw `Value`,
  recognises only `type == "call_tool"` (`:153-154`), and evaluates policy over `arguments` with
  `input` as the fallback (`:166-169`) — the legacy field names, which carry no truncation marker
  in either format. It never looks at `truncations`. So plumbing `TruncationMeta` is not one field
  read away: a V2 `tool_call` record is not counted by that branch today, and a slice that wants
  the marker must adapt between the two formats explicitly rather than read `truncations` off a
  legacy record where it does not exist.
- **A capture-source field exists on the other coverage path.** `coverage_report_v1.run.source`
  (`assay-cli/.../coverage/report.rs:166-173`) is set to `"decision_jsonl"` by
  `mcp wrap --coverage-out`, i.e. that sub-path knows it was proxy-observed.
- **The construction exists, and `assay-core` cannot name it.** `assay-evidence` sits in
  `assay-core`'s normal dependency closure only transitively, through `assay-adapter-api`
  (`cargo tree -p assay-core -e normal`); `assay-core/Cargo.toml` lists `assay-adapter-api` and
  `assay-common` alone (`:24-25`), and `assay-adapter-api` re-exports only its canonicalisation and
  shape helpers (`src/lib.rs:11-12`), not `coding_agent_claim_decision`. A transitive edge does
  not make a path nameable. Reaching the construction from `assay-core` therefore needs one of two
  things first: a direct `assay-core → assay-evidence` normal dependency, or a re-export through
  `assay-adapter-api`'s public surface. Either is a dependency-graph or API-surface decision, and
  neither is taken here. The edge to the runner substrate is missing as well
  (`cargo tree -p assay-core -e all` lists no `assay-runner-schema`).

What the policy path has is genuinely good: the denominator is declared (`total_tools_in_policy`,
`total_rules` come from the policy) and the residue is named (`unseen_tools`, `untriggered_rules`,
`high_risk_gaps`).

**And it already makes bounded-negative claims, with no stated basis, that fail runs.**
`high_risk_gaps` is *"Tool is in deny list but never appeared in test traces"*
(`coverage_next/analyzer.rs:216-226`) and drives `clean_pass = false` in
`coverage/legacy.rs:318-325`. `unseen_tools` and `untriggered_rules` are absence statements of the
same kind. None of them says whether the trace capture could have seen the tool.

Two further facts about `legacy.rs`, measured because the first draft got this example wrong:
unparsable lines **abort** the command (`:131`, `.context("invalid jsonl")?`), so a silently-skipped
trace is not the failure mode; and `rules_triggered` is constructed as an empty set (`:150`) and
never inserted into, so the triggered-rule count on this path is structurally zero. That is not the
same as a structural 0%: `rule_coverage_pct` divides only when the policy declares at least one
rule and returns `100.0` for an empty rule set (`coverage_next/analyzer.rs:209-213`; rule ids come
from `policy.sequences`, `:47-49`). So a policy with sequence rules reads **0% rule coverage
whatever the traces contain**, and a policy with none reads **100%** — full marks for a measurement
that was never taken. The empty-trace guard (`:210`) and the all-empty warning (`:219-221`) are as
the first draft described.

## Decision

### 1. What is shared is the lattice, the claim-kind enum, and the invariant. Not a table.

The CLAUDE.md admission test — *a mechanism whose second implementation would silently mean
something different* — is met for the two enums, with evidence rather than argument: the parity
test exists because the evidence-side table's first draft diverged. It is **not** met for the
tables, which differ by design (health half versus class half), and a "shared table" would merge
four readings of `Degraded` into one word.

### 2. What moves: the two enums only. Not the fold, not `permissiveness`, not any table.

A new `assay_common::claim` module holding exactly:

- `ClaimDecision{Allowed, Degraded, Blocked}`
- `ClaimKind{PositiveExistence, ExhaustiveSet, BoundedNegative}`

both `#[serde(rename_all = "snake_case")]`, both deriving what the existing pair derives, and
**neither deriving `Ord`**. `claim_parity.rs`'s refusal stands as written: the one comparison the
tree needs stays a free function next to the invariant that needs it.

This move explicitly authorises one new normal workspace edge:
`assay-runner-schema -> assay-common`. The runner-schema crate is a leaf today
(`CLAUDE.md:132`), but keeping its published type paths while making the common definitions their
single source requires that edge; an alias or re-export cannot exist without it. The migration must
update `CLAUDE.md` and the generated dependency graph in the same slice so that neither continues to
describe `assay-runner-schema` as a leaf. No other new dependency edge is authorised by this
decision, and this edge does not authorise moving a runner-domain table or reading into
`assay-common`.

The fold stays in `assay-evidence`. It folds the ceiling ladder, which is a source-class reading,
and it consumes a decision carrying a `String` rule — so moving it would move a domain reading and
require `alloc`. The first draft claimed the opposite on both counts and was wrong.

`assay-common` is `#![no_std]`; the root `serde` is `default-features = false` with `derive`, and
`cargo check -p assay-common --no-default-features` passes today. Two fieldless serde-derived enums
are expected to compile under `no_std`, and nothing checked in demonstrates it yet: the migration
slice must show it by re-running that same check after adding them. This is the **first** ungated
serde derive in
`assay-common` — `exports` is std-gated — and the precedent for an ungated module is `tool_pattern`,
admitted because *"every operation is a `&str` method and nothing allocates"* (`lib.rs:20-21`).
The lattice is the same shape.

### 3. `assay-mcp-server` must not construct lattice values.

After the move, `assay-mcp-server` (which already depends on `assay-common`, lists `assay-evidence`
only as a dev-dependency, and has it in its normal closure only transitively through `assay-core`)
*could* emit `ClaimDecision` values directly. It must not. `verify_side_effects.rs:162-164` placed
the `SideEffectLevel → source class → decision` composition in the CLI deliberately, and a lattice
value emitted without a stated basis is the exact thing the lattice exists to forbid. The enum
moving does not move the right to write it.

### 4. The policy/trace path: name the floor, name the debt, wire nothing here.

The first draft refused to wire the policy path on the ground that the only options were to default
the missing input or to return "no basis" for everything. The review found a third option, and it
is the honest one: a bare trace file's floor is `SelfReported` — `PositiveExistence: Degraded`,
`ExhaustiveSet: Blocked`, `BoundedNegative: Blocked` — and a floor is a declared worst case, not an
invented value. It is the same move `verify_side_effects.rs:190` already makes mapping
`SideEffectLevel::Asserted → SelfReported`. The `mcp wrap` sub-path, which knows its capture
source, can state a higher basis.

**Decided here:** `high_risk_gaps`, `unseen_tools` and `untriggered_rules` are bounded-negative
claims currently emitted **above** that floor with no basis. That is recorded as debt by this ADR
and is not left to look intentional. A follow-up slice must either base them (plumb
`TruncationMeta` and `run.source` into a capture-basis on `TraceRecord`, adapting the reader to the
V2 `tool_call`/`args` shape explicitly since it reads the legacy `call_tool`/`arguments` shape
today; count traces considered, contributing and skipped; fix `rules_triggered`) or demote them to
the floor. Which of the two is that slice's decision, not this one's, because it touches the trace
schema, a published wire contract with its own versioning.

**Not authorised here:** any new dependency edge from `assay-core` to `assay-runner-schema`, and
equally no new direct edge from `assay-core` to `assay-evidence` and no re-export through
`assay-adapter-api`. The construction a policy-path slice would map onto is
`coding_agent_claim_decision`; reaching it from `assay-core` takes one of the two routes named in
§Context, and taking one is that slice's recorded decision, not a side effect of wiring.

### 5. The parity tests are not deleted by the migration. Their debt is named.

`claim_gate_parity.rs` stays necessary after the move, because three tables remain three tables;
what the move retires is only its enum-equality half. `claim_support_parity.rs` is same-crate and
unaffected. Two gaps are recorded and not fixed here: the fidelity↔evidence pair is unpinned, and
the complete-coverage leg is dead. The migration slice must update both headers to name this ADR;
this branch adds the ADR only and leaves both headers as they are.

### 6. Public names are preserved; the semver gate must be made to cover both crates first.

The four existing enum types are public. The migration keeps all four paths resolving through
re-exports or aliases onto `assay_common::claim`: `assay_runner_schema::ClaimGateDecision`,
`assay_runner_schema::CoverageClaimKind`, `assay_evidence::CodingAgentGateDecision`, and
`assay_evidence::CodingAgentClaimKind`.
`cargo semver-checks` runs in `.github/workflows/split-wave0-gates.yml` (`check-release` against
the last `v*` tag) for assay-common, policy, metrics, core, registry and evidence — and **not for
`assay-runner-schema`** (`:459-476`, the `run_semver_for` allowlist). So the acceptance condition
is un-runnable for one of the two crates today. Adding `assay-runner-schema` to that matrix is a
prerequisite of the migration slice. If the check then reports a major, this follows ADR-047's
precedent and waits for the next major.

Neither enum is `#[non_exhaustive]`. The move is the one moment to decide whether the shared
lattice should be; this ADR says **no** — a fourth member is a change to what every table means
and must amend this ADR, so it should cost a major, not be absorbed silently.

## Alternatives considered

| Option | Why not |
|---|---|
| `assay-evidence → assay-runner-schema` as a real edge | The runner substrate is documented internal and API-unstable, and the edge would put runner-domain vocabulary in the evidence crate's public dependency surface. `claim_gate_parity.rs` names this and rejects it for the same reason. |
| A new tiny lattice crate | A twenty-third workspace package for two enums, when `assay-common` already exists for exactly this admission test and already holds a no_std precedent. |
| Status quo with parity tests | The honest baseline, and the reason this ADR is refactoring under no defect pressure. Rejected on drift risk alone — and the parity tests are both what makes the risk bearable and the evidence it is real. |
| Move the tables too | Rejected in decision 1: they differ by design, and a shared table would silently merge four readings of `Degraded`. |

## Why ADR-046 does not decide this

ADR-046 kept two reason-code registries separate. Its test was that they *answer different
questions on different surfaces*, and its reopening condition — *"One shared member is a
coincidence; two is a vocabulary"* (`:156-157`) and *"the case for revisiting it is a second
shared member, not a second opinion"* (`:77`) — was about one string shared across two
artifact-writing registries.

That test lands the other way on overlap: the overlap here is not one member but two complete
enums. It does **not** land the other way on surface — the three tables write different artifact
fields, which under ADR-046's own table is "different surfaces". The distinction that resolves
this: ADR-046's surface concern was a *consumer* keying on a shared string across artifacts. Here
the shared thing is a *type* consumed in-process, and a type identity split has no consumer-facing
analogue of a `ruleId` collision. So ADR-046's restraint about inputs carries over intact — it
refused to move `ReasonCode` into a crate that did not own the output, and decision 2 refuses to
move any table for the same reason — while its overlap test is applied, not stretched.

## Consequences

### What this buys

One definition of what `Blocked` means, so a consumer reading a runner report and an evidence pack
reads one vocabulary; the claim-class axis becomes citable as one thing; and a policy-path slice,
when it has a basis, maps onto the lattice instead of authoring a sixth table.

### What it costs

A published-crate migration across two crates, one new normal
`assay-runner-schema -> assay-common` edge plus the corresponding dependency-map updates, gated on
a semver check that must first be extended to cover one of them, for a mechanism that is currently
correct and parity-pinned. Stated plainly: the argument is drift risk, not a live bug.

### What would reopen or block this

- `cargo semver-checks` requiring a major for every alias shape → wait for the next major.
- A domain needing a fourth `ClaimDecision` member → amends this ADR; costs a major by decision 6.
- `assay-mcp-server` acquiring a need to emit a claim → that is a basis question and reopens
  decision 3, not a wiring change.

### What the reviews corrected

An adversarial review of the first draft, verified against source before being accepted, changed:
the title and decision 1 ("one rule" → one lattice and one invariant over three tables); decision
2 (the fold does not fold the lattice, needs `alloc`, and moves a source-class reading — it stays;
`permissiveness`'s anti-`Ord` refusal is now cited and kept); the site count (five tables and two
overrides, not three constructions); the policy-path measurement (`TruncationMeta` and
`run.source` exist unplumbed; `assay-core` already reaches a construction; the path already emits
unbased absence claims that fail runs; unparsable lines abort; `rules_triggered` is never
populated); decision 5 (neither parity test is deleted; `claim_support_parity.rs` is same-crate);
decision 6 (the semver gate does not cover `assay-runner-schema`); the ADR-046 reading (surface half
does not land the other way; quotes were blended); and the addition of decision 3 and the
alternatives table, both absent from the first draft.

A second, exact-head review of that revision, verified against source the same way, changed five
things and two smaller ones. The central table: a partial descriptor and `Partial` coverage yield
`Degraded` for `ExhaustiveSet`, not `Blocked`, on both the coverage and the evidence site, and the
prose about which tables differ now follows the measured cells. The reachability claim: the first
review's "`assay-core` already reaches a construction" was itself wrong — the edge is transitive
and un-nameable, and the real prerequisite is stated in §Context and decision 4. The
`TruncationMeta` bullet: the reader is on the legacy `call_tool`/`arguments` shape and the marker
is on the V2 `tool_call`/`args` shape, so plumbing requires explicit format adaptation. The rule
coverage claim: "structurally 0%" holds only when the policy declares rules; an empty rule set
reports 100%. The site inventory: three literal `CallRow` initializers in `verify_side_effects.rs`
were missing under a heading that said "every site". The smaller two: decision 3 now states
`assay-mcp-server`'s actual relation to `assay-evidence` (dev-dependency; transitively present via
`assay-core`), and decision 6 cites the semver allowlist block rather than the change-detection
line.

A third pass, on a full-base review, changed two things: the §Context statement of what the tree
has and lacks now rests only on checked-in tests and source rather than on a private working file,
and decision 5 states the parity-header update as a requirement on the migration slice rather than
as something this branch did.

A fourth pass, on 2026-09-03, combined two independent exact-head reviews. One caught that §Context
named the wrong pair as pinned: `claim_gate_parity.rs` pins `CoverageDescriptor::claim_decision` to
`coding_agent_claim_decision`, never `RunnerClaimGate`, which is pinned only within its own crate
by `claim_support_parity.rs` — the same fact §"The parity tests pin less than their names suggest"
already stated, contradicted three sections earlier. The other caught two things: the `cargo tree`
edge syntax was one cargo rejects, and decision 2 rested a `no_std` compile claim on a scratch crate
nobody can inspect; it is now a requirement on the migration slice.

A fifth pass caught that preserving the published `assay-runner-schema` names through aliases or
re-exports necessarily creates a normal `assay-runner-schema -> assay-common` edge, while the draft
still described the schema crate as a leaf and never authorised that edge. Decision 2 now authorises
that edge only and requires the migration to update both `CLAUDE.md` and the generated dependency
graph. A sixth pass caught that the correction itself was missing from this supposedly complete
review history; this paragraph closes that provenance gap without changing the decision.

A seventh pass caught that decision 6 said "All four crates" without an antecedent and then named
only the two public decision-enum paths. It now names all four public enum paths, including both
claim-kind paths, so compatibility cannot be read as preserving only half of the moved surface.

### Left open, deliberately

The capture-basis design for `TraceRecord`, and the base-or-demote choice for the policy path's
existing absence claims (decision 4). Nothing in this ADR changes runtime behaviour.
