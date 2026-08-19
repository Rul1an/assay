# Assay conformance corpora

One front door over every published corpus in this workspace, plus what each one
does and does not establish.

```bash
python3 conformance/run_all.py               # stdlib suites only, no toolchain
python3 conformance/run_all.py --with-cargo  # also the Rust-driven corpora
python3 conformance/run_all.py --json        # machine-readable report
```

Standard library only. No Assay import, no pip install, no network.

## The corpora

| Corpus | Vectors | Runner | Maturity |
|---|---|---|---|
| [`privileged-mcp-action-v0`](privileged-mcp-action-v0/) | 14 (5 accept, 9 reject) | **needs a candidate** | frozen, digest-pinned, open reproduction request ([#1840](https://github.com/Rul1an/assay/issues/1840)) |
| [`mcp-jsonrpc-id-conformance`](../examples/mcp-jsonrpc-id-conformance/) | 3 | stdlib, `check.py reproduce` | published pack; carries a positive control |
| [`rfc8785` canonicalization](../crates/assay-canonical/tests/vectors/rfc8785.json) | 31 | `cargo test -p assay-canonical --test rfc8785_conformance` | prerequisite vectors; vendored byte-identical into the clean-room pack |
| [`mcp-era-parity-v0`](../crates/assay-core/tests/fixtures/mcp-era-parity-v0/) | 18 (+2 equivalence pairs) | `cargo test -p assay-core --lib mcp::era_parity_tests` | **exploratory** — deliberately lower than the frozen corpus |
| [`observed-effect-v0`](https://github.com/Rul1an/observed-effect-v0) | — | its own repository | published separately; stdlib recompute + `corpusDigest` |

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

Exit codes: `0` nothing disagreed and nothing was left unproved · `1` at least
one suite graded `false` · `2` at least one suite graded `unproved`.

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

| Class | Meaning |
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
