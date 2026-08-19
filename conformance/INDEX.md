# Assay conformance corpora

One front door over every published corpus in this workspace, plus what each one
does and does not establish.

```bash
python3 conformance/run_all.py               # stdlib suites only, no toolchain
python3 conformance/run_all.py --with-cargo  # also the Rust-driven corpora
python3 conformance/run_all.py --json        # machine-readable report
```

Standard library only. No Assay import, no pip install, no network.

The runner's own behaviour is tested in `conformance/tests/test_run_all.py`
(`python3 conformance/tests/test_run_all.py`), including every path that must
grade `false` or `unproved` rather than pass silently.

## The corpora

| Corpus | Vectors | Runner | Maturity |
|---|---|---|---|
| [`privileged-mcp-action-v0`](privileged-mcp-action-v0/) | 14 (5 accept, 9 reject) | **needs a candidate** | frozen, digest-pinned, open reproduction request ([#1840](https://github.com/Rul1an/assay/issues/1840)) |
| [`mcp-jsonrpc-id-conformance`](../examples/mcp-jsonrpc-id-conformance/) | 3 | stdlib, `check.py reproduce` | published pack; carries a positive control |
| [`rfc8785` canonicalization](../crates/assay-canonical/tests/vectors/rfc8785.json) | 31 | `cargo test -p assay-canonical --test rfc8785_conformance` | prerequisite vectors; vendored byte-identical into the clean-room pack |
| [`mcp-era-parity-v0`](../crates/assay-core/tests/fixtures/mcp-era-parity-v0/) | 18 (+2 equivalence pairs) | `cargo test -p assay-core --lib mcp::era_parity_tests` | **exploratory** — deliberately lower than the frozen corpus |
| [`observed-effect-v0`](https://github.com/Rul1an/observed-effect-v0) | 14 cases | its own repository | published separately; stdlib recompute + `corpusDigest`. Adequacy measured — see below |

Related but not a corpus in this table: [RGE-Bench](https://github.com/rge-bench/rge-bench)
is maintained in its own repository under its own machine-checked neutrality guard,
and carries its own [`REPRODUCTIONS.md`](https://github.com/rge-bench/rge-bench/blob/main/REPRODUCTIONS.md).

## How a suite is graded, and why two values are not enough

A boolean cannot tell a reader whether a check ran and disagreed or was never
reached, and those need different repairs.

| Grade | Meaning |
|---|---|
| `proved` | the suite ran and agreed with its own pinned expectations |
| `false` | the suite ran and **disagreed** — a real, reportable divergence |
| `unproved` | an execution condition stopped the evaluation |

`unproved` is only ever produced by an execution state the runner observed: a
missing toolchain, an unreadable corpus, a non-zero exit with no parseable
report, or a test filter that selected nothing. It is **never** inferred from a
primary check that ran and failed, because that would report more than the run
established. Where a run mixes states, the worst one wins.

Exit codes, with `false` taking precedence over `unproved`:

| Code | Condition |
|---|---|
| `0` | no suite graded `false` and none graded `unproved` |
| `1` | **at least one suite graded `false`**, regardless of any `unproved` |
| `2` | no suite graded `false` and at least one graded `unproved` |

A run with both a `false` and an `unproved` suite exits `1`: a checked disagreement is a
stronger, more actionable result than a check that could not complete.

## Suites that do not run, and why that is not a pass

Three states are **declared, never inferred**, and always printed in the summary:

- `needs_candidate` — `privileged-mcp-action-v0` is a clean-room gate.
  `score_candidate.py` requires `--entrypoint`, an outside implementation. There
  is no self-run and deliberately so: a corpus that scores itself answers a
  question nobody asked.
- `not_selected` — a Rust-driven corpus when `--with-cargo` was not passed.
- `external` — the corpus lives in another repository.

"The suite agreed" and "nothing exercised the suite" must never print
identically. That is the same rule the corpora themselves enforce on the
implementations they grade, applied to the runner that grades them.

## What a green run does and does not establish

A green run says the published corpora reproduce **their own** pinned verdicts
on this checkout. It says nothing about:

- **independence** — everything here was authored in this workspace. Agreement
  with yourself is not evidence.
- **interoperability** — no cross-implementation fixture is exercised.
- **completeness** — outcome coverage is not rule coverage. A corpus can reach
  every declared outcome while a rule never decides anything, because another
  rule reaches the same outcome first on every vector it would have caught.
  Mutation adequacy is the criterion that catches that; RGE-Bench measures it
  with `scripts/check_rule_liveness.py` and the corpora in this table do not yet.

## Claim vocabulary

Where a reproduction is recorded, it carries an
[ACM Artifact Review and Badging](https://www.acm.org/publications/policies/artifact-review-and-badging-current)
class, because the distinction is easy to blur and ACM already drew it. Note the
terms are the reverse of the common intuition:

**These one-line glosses are abbreviations, not the criteria.** The badges carry
requirements this table does not restate: *Artifacts Available* needs an archival
repository, a unique dereferenceable identifier and a plan for permanent
accessibility; *Artifacts Evaluated — Functional* additionally needs the artifact
to be documented, consistent, complete and exercisable, so running a checker over
shipped vectors does not on its own earn it. Read the criteria at the source
before claiming a badge; the column below only says which distinction is meant.

| Class | Distinction meant here |
|---|---|
| Artifacts Available | the artifact is published and permanently retrievable |
| Artifacts Evaluated — Functional | the artifact runs and produces its own stated result |
| **Results Reproduced** | a different team obtained the result, **using** the author's artifacts |
| **Results Replicated** | a different team obtained the result **without** the author's artifacts |

Running a published checker over its own shipped vectors is *Functional*.

**Where the ACM frame stops short, stated rather than glossed.** ACM assumes the
author-supplied artifact is the author's *code*, so *Replicated* means obtaining
the result without it. A conformance corpus inverts that: the corpus IS the
artifact, and no reproduction can avoid using it. An outside party who writes an
implementation from the specification text alone, imports none of the author's
code, and recomputes every expected outcome from the inputs is therefore still
*Results Reproduced* under a strict reading, because the vectors came from the
author. That is the class this project claims, and it is the defensible one.

The fact worth reporting has no ACM badge: **independent implementation,
author-supplied vectors, expected outcomes recomputed from inputs alone.** That
is what [#1840](https://github.com/Rul1an/assay/issues/1840) asks for, and it is
more informative than either badge on its own.

## Mutation adequacy, measured on ourselves

[`corpus-adequacy`](https://github.com/corpus-adequacy/corpus-adequacy) asks a different question from the runner above:
not *do these corpora reproduce their own verdicts*, but **can an implementer
delete a declared rule, still reproduce the pinned outcomes, and be
indistinguishable from a conforming implementation?** A surviving mutant is a
hole in the contract rather than a gap in confidence, so the bar is 100% of the
rules the author declared.

The results below include the ones that do not flatter us, because a tool that
audits corpora is worth nothing if its author has not survived it.

**"Unexercised" below always means unexercised *by the published corpus*, never
unexercised in our tree.** The two come apart, and the difference is the whole
point: a rule our unit tests cover but our vectors cannot discriminate is one our
implementation gets right and our corpus cannot ask a stranger to get right. Such a
rule is a hole in what the corpus *transmits*, not a defect in what we ship, and
the rows below say which of the two they mean.

| Corpus | Runner | Result |
|---|---|---|
| `mcp-jsonrpc-id-conformance` | module | **6 of 10 in scope (60%), 4 survivors.** The first score was **4 of 4** over the presence/null arms alone. The positive control is a string id and RequestId is string or number, so the type arms belong in the denominator: string is isolated, number and bool are not. **7 envelope rules unexercised and out of scope**, ratio 0.7 — re-checked against the three published messages; none of them moves an outcome, and the reasons still hold |
| `privileged-mcp-action-v0` | process | **4 of 8 in scope (50%), 4 survivors, 1 known hole.** First measured as *no result*: three rules declared, two of them mutating the wrong stage and the wrong profile. That was a fact about the declaration, not the corpus — eight more rules were read off the verifier path and half of them survive. The survivors form **one systematic gap, not four**: wherever a rule has sibling members, the corpus discriminates exactly one and leaves the rest free. Decision cardinality is caught (`bad-103`) and observation and establish cardinality are not; the decision's `target_digest` is caught (`bad-102`) and the observation's `response_digest` is not; the binding pair is caught on its digest leg (`bad-105`) and not on its tool-name leg; the marker triple is caught on schema and code and not on origin (the known hole). One vector per failure mode, and every failure mode has siblings on the same code path that never got one. **This is the corpus carrying our live reproduction request ([#1840](https://github.com/Rul1an/assay/issues/1840))** |
| [`observed-effect-v0`](https://github.com/Rul1an/observed-effect-v0) drift consumer | batch | **14 of 23 in scope (60.9%), 9 survivors.** The first score was **4 of 5** over merge-policy rules only. The 14 case names announce the recompute, advisory, and profile rules as well. The original survivor remains (the body can be read whether or not the ref recomputed); the new ones are the fail-closed recompute siblings the cases never present, the RFC 8785 UTF-16 key-order rule the consumer claims but no body isolates, and a redacted-field conjunct that `incomplete_missing_non_claims` does not distinguish from a missing field |
| [rge-bench](https://github.com/rge-bench/rge-bench) | module | **51 of 54 in scope (94.4%), 3 survivors, 2 declared equivalent, 1 out of scope.** The first score was **30 of 30** over the hand-written table in `scripts/check_rule_liveness.py`. [ADMISSION.md](https://github.com/rge-bench/rge-bench/blob/main/ADMISSION.md) already says that table is not checked against the rules `ref_example.py` contains. The missing rules were discriminable: the strength ladder (the ceiling ladder's sibling — the same shape that hole already had), the AND conjuncts scored as one mutant, and the soft-digest and replay-equality fallthroughs. The three survivors include a claimed contract-edge (`True` is not `1`) that no vector isolates |
| `rfc8785` canonicalization | batch (test-names) | **control killed** — the corpus bites. No rules declared: the wrapper still has nothing of ours to delete. `to_string` is a second convenience over the same delegate and is not on this corpus's path |
| `mcp-era-parity-v0` | — | **not measurable today.** Driven by Rust *tests* without a per-vector verdict |

**Four measured, one control-only, one not measurable.** Stated rather than
rounded up — and this line has now been wrong in both directions on the same day.
It read *four measured* while `privileged-mcp-action-v0` was scoring nothing; it
was corrected to *three*; and it is four again only because that corpus was then
declared properly and finally produced a number. The number is 50%, which is the
worst result on this page. Both corrections are left visible, because a table
whose purpose is to publish unflattering results should show its own edits.

The same under-declaration then showed up on the other three scored rows. Each
first score was a number about the rules the author happened to list, not about
the rules the implementation has: 4 of 4 over four of ten id-field arms; 4 of 5
over merge-policy while the 14 cases name recompute and profile; 30 of 30 over a
hand-written table that ADMISSION.md already warned was unchecked against
`ref_example.py`. The rows above now score the larger denominator. The numbers
went down. That is the point.

### The v0 survivors are one gap wearing four faces

Worth stating separately because it changes what closing them costs. The four
survivors in `privileged-mcp-action-v0` are not four independent oversights; they
are four instances of the same construction habit. Every one of them is a *sibling*
of a rule the corpus does catch, sitting on the same code path:

| the corpus discriminates | it does not discriminate | shared path |
|---|---|---|
| `decisions.len() != 1` (`bad-103`) | `observations.len() > 1`, `establishes.len() > 1` | Stage 2 cardinality |
| decision `action.target_digest` (`bad-102`) | observation `caller_visible_response_digest` | `is_sha256_digest` |
| binding on `target_digest` (`bad-105`) | binding on `tool_name` | the same `&&` chain |
| marker `schema` + `code` | marker `origin` (the known hole) | the same triple match |

The corpus was built one vector per failure *mode*. Each mode's siblings share the
structure but never got a vector, so an implementer can drop the second member of
any of these pairs and reproduce all fourteen outcomes.

That the pattern is uniform is the useful part: it says the fix is not fourteen
judgement calls but one rule — **for every conjunct and every sibling member, ask
whether a vector isolates it**. That audit was then run against the other scored
rows on this page, independently of what each manifest declared. The same habit
showed up: mcp-jsonrpc-id isolated presence and null and left number and bool
free; the drift consumer isolated digest-mismatch and left malformed / unresolved
/ redacted free; rge-bench isolated the origin-ceiling ladder and left the
strength ladder undeclared until this measurement. Note that our own tree
catches the v0 four; this is about what the corpus can transmit, not about what
we ship.

Not fixed here. Every one of these needs a new vector, and a new vector moves a
digest that [#1840](https://github.com/Rul1an/assay/issues/1840) has published as a
reproduction target. Whether that is answered by an addendum corpus at a second
digest, or by leaving v0 pinned and honest, is a contract decision rather than a
tooling one.

### Why `rfc8785` has a control and no declared rules

`crates/assay-canonical/src/jcs.rs` is a thin wrapper: it delegates to `serde_jcs`
and declares almost no rules of its own. Rule-deletion mutation therefore does not
apply the way it does to a corpus with its own logic — there is nothing of ours to
delete. The question for a delegating wrapper is different: **does the corpus catch
a wrong delegate?**

That control was not invented for this measurement. `tests/rfc8785_conformance.rs`
documents it in prose at the top of the file: swap `serde_jcs::to_vec` for
`serde_json::to_vec` and at least 8 of 31 vectors must fail, with
`keyorder_utf16_vs_codepoint` among them, because that is the only vector where
code-unit, code-point and byte order disagree. The file says the property is
written down "so it stays checkable rather than remembered".

**It was remembered, not checked.** Run for the first time on 2026-08-19 it holds
exactly: `8 of 31 RFC 8785 vectors diverged`, and the named vector is among them.
That is now executable rather than a paragraph.

What the control does **not** establish is coverage of RFC 8785 itself. It shows
the corpus bites against **one** wrong implementation. Whether it bites against the
other plausible ones is a question about RFC coverage rather than about our code,
and it is open.

Every manifest must declare at least one **control** — a mutation on the same
path that MUST be killed. All-survivors because a corpus is weak and
all-survivors because nothing was ever measured print identically, so without a
control a zero says nothing. Controls are excluded from the score, and a control
that survives fails the run with every other verdict declared meaningless.

### Where the adequacy tool lives, and why not here

The measurement tool is **not vendored in this repository**. It lives at
[`corpus-adequacy/corpus-adequacy`](https://github.com/corpus-adequacy/corpus-adequacy),
in its own organisation, and this repository keeps only the **manifests** that
describe our corpora to it.

```bash
git clone https://github.com/corpus-adequacy/corpus-adequacy   # as a sibling of this repo
python3 ../corpus-adequacy/corpus_adequacy.py conformance/adequacy/<name>.manifest.json
```

Two reasons, and the second is the one that decided it.

**One implementation.** Two copies of a measurement drift, and the copy that
drifts is the one that stops measuring. That is the same rule this workspace
applies to canonicalization and to DSSE pre-authentication encoding. The
duplication had already begun within a day: the vendored copy and the extracted
one disagreed on their schema ids before either had been used in anger.

**An instrument offered to peers may not be vendor-named.** The schema id was
`assay.corpus_adequacy.manifest.v0`, and it is now `corpus-adequacy.manifest.v0`.
That id ends up inside another project's manifest. `rge-bench`'s own
`check_public_surface.py` — a neutrality guard written in this workspace —
rejected the vendored id on exactly that ground, which is a stronger signal than
any outside comparison because it is our own rule applied to a case we had not
foreseen.

`conformance/bounded_run.py` stays here and is duplicated upstream. It serves
`run_all.py`, which is this repository's own runner, so both copies have a live
local caller. That is a known duplication rather than an unnoticed one.
