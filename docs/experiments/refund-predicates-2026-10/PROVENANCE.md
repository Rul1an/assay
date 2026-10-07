# Package provenance

This records construction and evidence boundaries, not approval to publish.

## Immutable input layer

`record/` uses the repository's existing convention for byte-preserved research
artifacts. Moving these files under that directory changes packaging paths only;
within the original retained tree, every relative path and byte remains intact.
The original inventory digest and result digest are enforced by the verifier.
No historical absolute path is opened, imported or executed.

`source-manifest.json` gives the 217 selected source/export identities from source
snapshot `725e5d441462f0ff888053dd817a8b8cf07f75db`. Measured C3 input identities
remain pinned separately by `record/mutation/SOURCE-PINS.json` at subject commit
`9d5a3da6069b5a89b0ca378034519310c10e0243`.

Two contract-only editorial changes were made:

- C1: replace its private scope-reference preamble with a neutral synthetic
  convention statement; all normative sections retained.
- C2: replace the historical design/review preamble with a neutral synthetic
  convention statement; all sections starting at the input packet retained.

Fixtures, malformed inputs, literal expectations, original source code and
measured evidence were not reformatted. The original C3 mutation input metadata
includes historical review filenames and opaque commit identities; these are
provenance labels, not accessible-review or authenticated-review claims.

The pinned CA MIT license is copied from instrument commit
`7f4c8785fedbe43cfceb1d3e8cb26c7028215d08`. Original exported instrument code is
inert retained evidence, not a dependency executed by the new verifier.

## New verifier and test evidence

The new verifier was derived from retained receipts/observations, the literal
oracle, frozen edits and the public CA serialization rules. Its author inspected
the original consumer for format context; it imports neither producer nor CA
code. It does not read or use prior reviewer derivation code. It independently
compares raw report claims, not independently re-executes the measurement.

The builder also authored C2's independent reproducer and cannot provide this
package's final non-building technical review. Model/writer names are declared
provenance, not authenticated reviewer identity.

Tests were written before the new modules. Initial missing-module failures are
setup RED only. Additional behavioral RED tests caught an unbounded count of
empty directories and acceptance of an overflowing numeric exponent; both were
fixed and passed. Test-only tamper copies exercise corrupted, missing and
symlinked receipts, extra files, process failure/type errors and missing evidence.
No mutation measurement was performed.

Focused verification uses explicit top-level test modules and the two named test
modules in each of C1, C2 and C3. It never discovers tests under the retained
instrument/source tree. The new modules are linted explicitly; the Ruff config's
only evidence exclusion is `record/mutation/retained/**`. Retained source bytes
must never be autofixed. Repository-wide Rust checks are not implied by these
Python example checks.

C2's separate retained baseline remains pending at this package revision. Passing
its focused tests does not fill that evidence gate. Final technical review,
privacy/safety review and release approval remain separate.

## Pending C2 verifier preparation

`BASELINE-PREPARATION.md` records the new offline format checks and their explicit
remaining integration work. This addition neither changes the original retained
bytes nor replaces the earlier package verification results. Its CLI remains
unconditionally pending and emits no accepted C2 record. The repaired carrier
format is a source reference only; there is no retained C2 measurement in this
revision.
