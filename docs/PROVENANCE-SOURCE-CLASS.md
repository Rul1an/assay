# Provenance: source class, coverage, and the conclusions they license

This file exists so that anyone comparing implementations can **check** dates rather than infer them.
Every claim below is a commit in a public repository. Nothing here asserts ownership of an idea; it
records when this implementation shipped which piece, and credits the work it builds on.

Verify any line with `git log`. If something here is wrong, that is a bug — open an issue.

## Prior art, first

Kernel-level vantage as a security-observability position is **not new here** and this project does not
claim it. [AgentSight (arXiv:2508.02736)](https://arxiv.org/abs/2508.02736) established system-level
agent observability via eBPF. The idea that signal sources differ in how far the observed workload can
tamper with them was published as a four-tier hierarchy by
[ARMO (Ben Hirschberg, 2026-05-22)](https://www.armosec.io/blog/what-to-instrument-for-ai-agents/):
tamper-resistant / partially resistant / cooperative / repudiable.

Runtime verification has had inconclusive verdicts under partial observation for decades; see
[arXiv:2604.26753](https://arxiv.org/abs/2604.26753). The `incomplete` conclusion below is that
result applied to evidence records, not an invention.

## What this repository shipped, and when

| date | what | commit |
|---|---|---|
| **2026-05-22** | `assay.runner.observation_health.v0` — a coverage carrier deliberately separate from enforcement health, because a run can have complete observation and absent enforcement | `8959b7bc` (PR #1317) |
| **2026-06-01** | `RunnerClaimGate` — **occurrence and absence gated separately**: under partial coverage `measured_positive_claims` is `Degraded` while `bounded_negative_claims` is `Blocked` | `d16a1f97` |
| **2026-06-04** | the runner **coverage-descriptor gate** — a missing or malformed descriptor blocks all claim kinds; partial coverage allows positive claims and blocks absence | `035d0fe3` (PR #1487) |
| **2026-06-24** | `CodingAgentSourceClass` — a typed observation-position axis with six values: `boundary_observed`, `independently_observed`, `third_party_observed`, `producer_reported`, `issuer_attested`, `receiver_receipt`. Plus `CodingAgentCoverageState` (`observed`, `unavailable`, `self_reported`, `absent`, `partial`) | `56c965bf` (PR #1754) |
| **2026-08-06** | `coding_agent_dimension_conclusion` and `coding_agent_claim_ceiling` — the two axes combined, coverage evaluated first, against the published five-rung ladder; `CodingAgentCoverageReport::weakest_ceiling()`, `meets()`, `gaps()` | this commit |

Sibling repositories, same axis:

| date | what | where |
|---|---|---|
| **2026-06-25** | `source_class_ceiling` shipped as a named scoring axis in the initial public commit, alongside `sufficiency`, `recompute`, `incomplete_visibility`, `tamper_fail_closed`, `format_equivalence` | [`rge-bench/rge-bench`](https://github.com/rge-bench/rge-bench) `2b2b4a5` |
| **2026-07-02** | issuer-vantage boundary vectors + ceiling readings mapping public record formats (in-toto agent-decision, SEP-2828 execution records, AAPR, MCP `evidenceRef`, and this project's own records, capped first) | `rge-bench` `e904606` (PR #10) |
| **2026-07-23** | 71 vectors / 11 axes reproduced byte-for-byte from inputs alone by an independent author on an unrelated stack (Spring Boot 4 / Jackson 3, [`JM-Lab/rge-bench-java`](https://github.com/JM-Lab/rge-bench-java)); digest `sha256:e769822b…` | `rge-bench` main |

## The rule the 2026-08-06 commit enforces

> A source class says **where an observer sat**. Coverage says **whether it looked**. Neither alone
> licenses a conclusion, and the weaker of the two binds.

Concretely, in `crates/assay-evidence/src/coding_agent.rs`:

- Coverage is evaluated **before** source class. An observer that did not watch has nothing to be
  right or wrong about, however well positioned it is.
- The four unobserved states keep distinct reasons — `not_observed`, `observer_unavailable`,
  `self_reported_only`, `partial_only` — because collapsing them loses why the conclusion is
  unavailable.
- A source class that looked resolves to its rung on the published ladder, unchanged from
  `rge-bench`'s: `asserted` < `asserted_signed` < `observed_at_receiver` < `observed_in_path` <
  `independently_confirmed`. A `receiver_receipt` that watched caps at `observed_at_receiver` and is
  **not** promoted to an independent class — an earlier draft of this commit flattened the ladder into
  a binary and over-granted exactly there; `a_receiver_receipt_is_not_promoted_to_independent` now
  pins it.
- Attestation never appears in the ceiling: signing raises tamper-evidence, never vantage.
- Across dimensions the **weakest rung binds**, and a coverage gap binds harder than any rung —
  `weakest_ceiling()` returns `None` rather than a low rung, because there is no rung to take a
  minimum with.
- A dimension the run never declared is **out of scope, not a gap**. An absent claim and an unmet one
  are different facts.

The payload still carries no verdict. `coverage_report()` answers one narrower question: *may a
consumer draw a clean conclusion from these facts at all.*

## Why this file exists

A ranking of sources without a coverage denominator reads "top-tier source, no event" as a clean pass.
That is the failure this primitive prevents.

**An earlier draft of this file claimed we were not aware of another implementation that enforces it.
That was too strong and is withdrawn.** [Vaara](https://github.com/vaaraio/vaara) enforces the same
principle at boundary granularity, and states it well — `src/vaara/integrations/_mcp_attest.py` stamps
a coverage block into every authorization receipt with the scope literal
`calls-routed-through-chokepoint`, because *"a tool reached on an out-of-band path is out of coverage,
so an absent receipt for it is silence, not a clean negative"*, and *"a reader tells an absent deny
apart from an unobserved one by reading it against this boundary"*. It pairs that with a gap-free
per-boundary sequence so a dropped receipt is a provable gap. That is the denominator idea,
implemented, and it is theirs.

Note also that the occurrence/absence asymmetry and the declared-descriptor requirement are not new
here either: they shipped in `assay-runner-schema` on 2026-06-01 and 2026-06-04 respectively, three
weeks before the source-class work, and today's `assay-evidence` primitive is a second implementation
of that rule in a different crate — recorded as a defect to reconcile, not as a feature.

What remains distinct here is narrower: their coverage is **one scope per boundary** — was this call
within the chokepoint at all — while this primitive carries **per-dimension coverage per run** and
combines it with the source-class ladder, so a claim needing a surface the observer did not watch
cannot clear even from the top rung. Both prevent absent-reads-as-clean; the granularity and the
composition with a ceiling differ. We would rather state that narrowly and be right than claim the
principle.

Ideas in this area travel quickly and often arrive somewhere without their history. That is normal and
mostly benign. This file is the cheap fix: a dated record anyone can verify, so nobody has to take a
priority claim on trust — including ours.

## Related work we build on or compose with

- [`neldan00077/custody-precondition-vectors`](https://github.com/neldan00077/custody-precondition-vectors)
  — capturer-identity provenance as a ceiling (`issuer_established` supports independent third party,
  `self_asserted` caps at same domain). A ceiling on a different axis; the two compose.
- `giskard09` and `babyblueviper1` in
  [a2aproject/A2A#1734](https://github.com/a2aproject/A2A/discussions/1734), 2026-08-02 — a
  verdict-of-verdict nesting rule, `min(own, inner)`, so a re-reviewer inherits the weakest vantage in
  a chain rather than the last one. That nesting extension is theirs and is not implemented here.
- [Proof of Execution (arXiv:2607.05397)](https://arxiv.org/abs/2607.05397) — states plainly that a
  recorded mutation which never occurred is outside its threat model, and marks its observation plane
  non-authoritative. Useful precisely because it is explicit about the boundary.
