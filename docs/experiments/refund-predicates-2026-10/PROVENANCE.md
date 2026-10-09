# Package provenance

This records construction and evidence boundaries, not approval to publish.

## Immutable input layer

`record/` uses the repository's existing convention for byte-preserved research
artifacts. Moving these files under that directory changes packaging paths only;
within the original retained tree, every relative path and byte remains intact.
The original inventory digest and result digest are enforced by the verifier.
No historical absolute path is opened, imported or executed.

The retained C3 run, formerly the directory `record/mutation/retained/`, is
stored as the single archive `record/mutation/retained.tar.gz`. As a directory
it added 7,185 entries (the directory, 85 subdirectories and 7,099 files) and put the repository
past the 10,000-entry materialization ceiling of the pinned adequacy tool.
Archiving changes the container only: every member keeps its original relative
name and bytes, which the verifier checks against the pinned inventory and
result digests whichever form it reads. Container facts, as generated twice
with identical bytes:

- members: 7,099 regular files; name, length and SHA-256 of every member equal
  the pinned source tree at assay `8b235dc85` (SHA-256 of the sorted
  `name<TAB>length<TAB>sha256<LF>` listing:
  `17f7def0fcb58161789eaea0aaa68186c24a4e3077ca29e42675b321b837b979`);
- uncompressed tar stream: 18,848,768 bytes, SHA-256
  `a2262cd4d957ecaf4b41e098c46aab14e01283f66feb1de79cce4500ce7a432c`;
- archive file: 1,862,919 bytes, SHA-256
  `609cfafd406c8ee5eb89c87305d9c368d1f8332d16f5838bc8bcfcd7a85f887d`
  (Python 3.14.3, zlib 1.2.12, compression level 9).

The compressed bytes depend on the zlib build. The uncompressed tar stream
does not: it is fixed by `pack()` in `evidence_io.py` (sorted names, ustar
headers with mode 0644, uid/gid 0 and mtime 0, no directory members).

The repository's evidence-vocabulary guard read the 7,099 retained files as
ordinary tracked text. Archiving must not end that, so the archive is not a
binary exception (an exception skips a file's contents). The guard lists it as
a scanned archive instead: it opens the archive through `evidence_io.py`'s
bounded reader and applies its usual rules to every member under the member's
former path, `record/mutation/retained/<member>`. A refused, malformed or NUL
member is a guard failure, not a skip.

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
evidence exclusions cover only `record/baseline-c2/**` and
`record/c2-carrier/**`; the retained C3 source is inside the archive, where no
linter or formatter reaches it. Retained source bytes
must never be autofixed. Repository-wide Rust checks are not implied by these
Python example checks.

## C2 retained baseline integration

The earlier preparation revision deliberately had no acceptance path. The current
verifier accepts only the pinned original baseline described in
`BASELINE-PREPARATION.md`. Its 228 retained files were copied unchanged, including
the original inventory and result/provenance hash relationship. Existing C1/C2/C3
frozen inputs and the prior C3 retained tree were not edited or remeasured.

`record/c2-carrier/source-map.json` records six byte-identical carrier sources from
`6bdeb4b10f3952ccd2fe6f792345f5a0d44efec4`. They explain source/context and resource
accounting without executing original code. The existing pinned CA MIT license
covers the new baseline export as well as the C3 export. No strategic notes or
prior reviewer implementation were included.

Actual integration RED recorded the prior pending refusal and absent default
CLI route. The first integration exposed native ordinary-declaration normalization
before schedule hashing; that rule was checked against the pinned public codec.
The integration suite verifies the full retained baseline and a relocated copy,
and rejects test-only altered context, dispatch, source/vector, pair and budget
records after inventory rehashing. Its verifier is newly derived; it does not
re-execute CA or authenticate historical custody or loaded process code.

The saved resource snapshot precedes late provenance/publication/retention writes.
Its historical free samples and largest boundary count are not current floor
proof or peak usage. Terminal logical totals are separately derived from retained
file sizes. Native null/unresolved instrument identity remains distinct from
external source-pin equality. Final technical review, privacy/safety review and
release approval remain separate.
