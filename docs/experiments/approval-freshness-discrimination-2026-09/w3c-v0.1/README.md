# The approval-freshness record in the v0.1 reporting format

> Experimental. A report of the run in the parent directory in the W3C Agent
> Conformance and Benchmarking CG's reporting format v0.1
> (public-agent-conformance 2026Sep/0087), with one question left open. The
> mapping is the author's and has not been reviewed by the group.

## Files

| File | What it is |
| --- | --- |
| `records.v0.1.json` | the report: four `pass` records, every qualifier `unknown`, and `nothing` in the 5.4 field |
| `mapping-proposal.json` | what the records would be under one reading of the open question; the report does not consume it |
| `check.py` | a limited evidence check, not a v0.1 validator; standard library only |
| `test_claim_boundary.py` | four tests that the report claims no reading |

```bash
python3 check.py --controls
python3 -m unittest test_claim_boundary
```

Both read only this directory and the parent record. They do not rerun the
tests; `../recompute.sh` does that.

## The report

The baseline run (`../observations/baseline.txt`) has four declared checks,
all `pass`. The aggregate is 4 of 4 declared and exercised checks; the
accounting, execution and no-void claims hold over a population of 4. Evidence
completeness does not hold under this report's reading of it, because no
record carries or references evidence. The 5.3 counter is 0 carried, 0
referenced. The run produced no fail, and the 5.4 field answers `nothing`.

The mutant run (`../observations/mutant.txt`), where the expiry test fails,
is recorded beside the report as an observation, not as a v0.1 evidence
object. Which slot it would fill is the open question.

## The open question

Both runs share the test source, the code that builds the fixtures, the
command, the toolchain and `Cargo.lock`. What differs is the implementation
under test, and with it the compiled test binary, because Rust builds tests
and handler into one executable. The fixture bytes also differ in their
timestamps, since the helper builds them from the wall clock.

The question: does the same test source, rebuilt into a new binary, count as
the same checker when only the implementation under test changes?

`records.v0.1.json` lists four readings the author considered (A to D) and
what each would permit. They are offered to help answer the question, not as
settled consequences. Only reading A would permit qualifiers or a 5.4
control, and `mapping-proposal.json` writes out what it would give.

## `check.py`

It checks that the cited record files match the digests in the report and in
`../SHA256SUMS.txt`, with the entry count bound (3.1), that every path named
exists, and that the per-test outcomes read from the observations match the
records. On the report it applies rows 1, 6 and 7, recounts the 5.1 aggregate
and the 5.3 counter, and checks the explicit 5.4 answer. For 5.2 it checks
only that a completeness population is present, not whether completeness is
satisfied. The 5.3 check covers this package's zero qualifiers, not general
reports with multiple qualifiers per record. Rows 8 to 13 have nothing to read there, because no qualifier is
asserted. On the proposal it checks section 2's closed `changed` vocabulary,
resolves `moved` from the two observations, and applies rows 8, 9, 11, 12, 13
and 14. `--controls` runs 28 altered copies, each of which must be rejected
for its own stated reason. None of this decides the open question.

## What this does not establish

Everything the parent record says it does not show still holds: that the
freshness rule or the test's expectation is right, anything about other
mutations, boundary behaviour, or a deployed system. The observations and the
recorded rerun are the author's own, on one machine; there is no independent
reproduction or oracle review.
