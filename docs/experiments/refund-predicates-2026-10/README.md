# Synthetic refund predicates and retained fault-discrimination evidence

Three offline examples ask different questions of synthetic packets. They keep
policy binding, declared effect evidence and client-visible knowledge separate.
The examples are research artifacts under `docs/experiments`; they do not extend
the Assay runtime or authorize a payment.

| Example | Question | Inputs |
|---|---|---|
| [C1](record/c1/CONTRACT.md) | Does a declared dispatch bind to a complete synthetic approval, and what does declared dispatch coverage establish? | 19 semantic packets and 23 invalid files plus a size-bound recipe |
| [C2](record/c2/CONTRACT.md) | Do admitted operations have at most one counting-committed effect under declared coverage and epoch closure? | 20 semantic packets, 25 operation rows and 29 invalid files |
| [C3](record/c3/CONTRACT.md) | Was definite support or expressed uncertainty justified by observations delivered when a client reported? | 30 semantic packets, 35 claim rows and 46 invalid files plus two boundary recipes |

Each directory has a reader, a separately authored reproducer, literal
expectations, original input hashes and focused CLI tests. The authors saw the
expected answers; this is not blind or external validation. C2 reversal does not
erase a counted effect, and matching workflow keys never establish attribution.
These predicates are not pooled into an overall verdict or score.

**Current local package status:** this package carries one original C2 baseline
and one original C3 mutation run, with offline verifiers for both. The focused
reader/reproducer tests remain separate evidence. Final package review and release
are separate from local verification.

## Run the examples

Python 3.11+ and a POSIX filesystem are required. No external packages, credentials,
network access or CA installation are needed for these commands. From this directory:

```sh
for slice in record/c1 record/c2 record/c3; do
  (cd "$slice" && PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest -v test_reader test_independent_reproducer) || exit
done
```

The commands exercise all frozen semantic/refusal cases and additional boundary
checks. They execute only local readers on synthetic packets. They do not run a
CA baseline, controls or mutations. For one C2 packet:

```sh
python3 -B record/c2/reader.py record/c2/fixtures/SINGLE-COMMIT.json
python3 -B record/c2/independent_reproducer.py record/c2/fixtures/SINGLE-COMMIT.json
```

Semantic results exit 0. Invalid packets exit 2 with stderr and no report.
`source-manifest.json` records original/export hashes and the two editorial
contract changes; fixtures and expectations were not edited. Source hashes are
byte identities, not endorsements or authenticated custody.

## Verify the original C2 baseline offline

```sh
python3 -B verify_baseline.py
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest -v test_verify_baseline
```

The pinned baseline contains 20 successful receipts and 25 operation rows, all
matching the frozen oracle. The positive control was declined before invocation
and the ordinary mutation was not run. The final observation closes the stopped
prefix without admission. This is a baseline comparison, not a C2 mutation score.

The verifier checks the full raw report/receipt/dispatch chain, source and context
bindings, complete retained graph, mandatory result/provenance pair and recorded
budget arithmetic. It executes no retained code and emits no partial result.
[Baseline verification details](BASELINE-PREPARATION.md) give the immutable pins,
relocation command, source map, resource snapshot limits and non-claims. The
original CA-native commit remains null/unresolved; its seven exported files bind
separately to the external source pin.

## Verify the original C3 mutation run offline

```sh
python3 -B verify_mutation.py
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest -v test_evidence_io test_verify_mutation
```

`verify_mutation.py` reads all retained files and raw reports. It does **not**
execute retained source, imports from the retained instrument, or any original
command. The retained run ships as one archive,
`record/mutation/retained.tar.gz`: a plain directory tree of its 7,099 files
would put about 7,200 entries into the repository, which is more than the
repository-wide materialization ceiling of the pinned adequacy tool allows.
The verifier unpacks it into a private temporary directory under the same
size, count, depth and path limits it applies to a directory, refuses links,
pax/GNU extensions, directory or device members, duplicate or overlapping
names and any data past the archive's end, and removes the unpacked tree
afterwards. Optional `--retained`, `--mutation-inputs` and `--c3-inputs`
arguments select other inputs; `--retained` takes that archive or a plain
directory tree with the same contents. Either way the verifier still requires
the exact pinned original inventory and result.
The public output is a bounded verification record, not a new measurement.
Missing evidence, malformed inputs, links, changed bytes or failed checks refuse
with exit 2 and no stdout. There is no partial-pass mode.

The original measurement used the public observation interface of
[Corpus Adequacy](https://github.com/corpus-adequacy/corpus-adequacy) at
`7f4c8785fedbe43cfceb1d3e8cb26c7028215d08`. Its seven original exported source files
are preserved as inert evidence under `run/export/` inside the retained archive; the pinned
MIT license and copyright are in [record/mutation/CA-LICENSE](record/mutation/CA-LICENSE),
covering both the C3 and C2 retained exports.
The verifier uses no CA API. This package grants no authority to resume sessions,
repeat the measurement, contact an external party or assert a result about an
external implementation. C1/C2/C3 focused-test results do not establish that CA
was used on those slices. The separately retained C2 baseline and C3 mutation
run support the bounded CA-use claims described here.

### What is checked

- All 7,098 inventoried original files, exact paths, sizes and hashes, with no
  extra or missing files; 3,961 content-addressed blobs.
- All 900 distinct process receipts and matching dispatch intents, successful
  return codes, empty stderr, source bindings, raw report bytes and slot links.
- Complete baseline/control/ordinary case coverage and full raw report identity;
  two baseline-only sessions plus seven completed patch sessions.
- Original subject bytes and each frozen single-anchor edit without executing an
  edit; positive/inert control outcomes; admitted prefix/context/decision and
  consumption/completion ledger bindings.
- Independent projection of all raw claim rows against the literal oracle,
  preregistered target movement, reason/diagnostic extras, unexpected axes, and
  exact equality with the original `result.json`.
- Saved budget-decision arithmetic. This is not a measurement of current memory,
  filesystem allocation or historical peak disk use.

The verifier author inspected the original producer consumer and public CA
serialization source to understand the formats. The verifier imports neither;
its projection code is newly written. Prior reviewer derivation scripts were
not used. Final technical review must be performed by a non-author.

### Retained result

The seven chosen edits have 23 target movements as predicted and zero unexpected
applicability/result movements. This is directed, exposed-answer discrimination
for those edits, not universal corpus adequacy.

| Edit | Target movements | Full-claim cases changed | Extra entries |
|---|---:|---:|---:|
| F1 occurrence treated as delivery | 5 | 7 | 7 |
| F2 future deliveries affect earlier reports | 4 | 5 | 6 |
| F3 coverage ignored | 6 | 5 | 6 |
| F4a coverage masks contrary evidence | 1 | 1 | 1 |
| F4b coverage masks conflict | 1 | 1 | 1 |
| F5 last terminal observation wins | 3 | 5 | 8 |
| F6 unjustified uncertainty accepted | 3 | 3 | 0 |

F5 includes three observed conflict changes on target rows that were not
separately preregistered as extras. They remain observed extras, not additional
predictions or detections. Gaps remain: no F4a conflict-subpath witness, no F5
support-axis witness, and only one witness each for F4a and F4b.

## C3 evidence identity and limits

- Execution source: `9e71e44dafeea412c5dbe16f363f281f9c00ed5d`.
- Measured subject/oracle: `9d5a3da6069b5a89b0ca378034519310c10e0243`.
- Patch freeze: `946fa075b92d06fe7baf597d278ab11347f9306c`.
- Retained inventory SHA256:
  `3e536b335a9532f388b3e61646b4f16d33be45391e56972046cce05f115a3e21`.
- Original result SHA256:
  `c26f5b562e2963a469540237591ad3f3ca24e8a1ef0027164efbe50f536b9625`.

The intact retained tree includes historical local path labels, device/inode
values and observed interpreter metadata. Those labels are inert provenance and
are never resolved or executed by the verifier. No current availability or
authority is implied. Preserving the originals keeps their hash relationships;
it does not itself constitute privacy approval for release.

Coverage, custody, ordering and approvals are synthetic declarations. Nothing
establishes a real refund, provider finality, issuer authentication, complete
runtime observation, independent custody, exactly-once behavior or refund
safety. No adequacy score, general defect diagnosis, external corroboration or
second measured reproduction is claimed. Instrument runtime source state remains
`unresolved`; export hashes bind bytes, not the host. Interpreter-slot identity
binds wrapper bytes; observed child path/version is not attestation.

The C3 historical budget covers logical evidence bytes, excluding temporary subject
copies, CA lock files, RAM and filesystem allocation. The largest saved boundary
value is not a peak, and no free-space samples were retained. Unsampled transient
writes depend on the original reservation argument; this offline verifier does
not turn that into a disk trace.

## Adapter report-selection follow-up

The separate [adapter-selection package](record/adapter-selection/README.md)
keeps the four LATE/LATE-CONFLICT reports distinct and records responses to
selection faults in the adapter path. It includes inspected source bytes,
literal mutation recipes and a standalone recount of retained results:

```sh
python3 -I -S -B record/adapter-selection/independent_reproducer.py record/adapter-selection
```

This command checks recorded results; it does not rerun the adapter or CA.
The package distinguishes guard refusals from claim changes, and records a
selection fault the four named reports alone do not expose. It concerns
pre-dispatch use of earlier evidence, not verification of the newly dispatched
action or the candidate federation-port v1 target contract. Its own README
states the omitted host evidence and limits of the recount. The input limits
below describe the original baseline/mutation verifiers, not this separate
recount program.

## Input limits

### Repeat the small checks first

From the repository root, run the five regression tests before considering a
new trial:

```sh
python3 -B -m unittest discover -s docs/experiments/refund-predicates-2026-10 -p 'test_adapter_*.py' -v
```

The required CI job runs this same command. `test_adapter_selection.py` calls
the retained projector's real pure selection function: the four named reports,
missing or duplicate identities, and a mismatched sequence. It is a producer
test. `test_adapter_recount.py` instead invokes the independent recount as a
black box, checks the recorded result, and rejects changed blob, receipt and
final bytes. These two kinds of evidence are separate. Neither executes CA,
the native reader or the full adapter.

The retained recount reports these comparison cells:

| Matrix | Unchanged (U) | Diagnostic only (U*) | Claim changed (SM) | Refusal to claim (AI) | Refusal changed (RO) | Projector refusal (GR-P) | Postcondition refusal (GR-A) |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| M | 102 | 2 | 0 | 7 | 17 | 6 | 6 |
| C | 31 | 0 | 6 | 17 | 2 | 0 | 0 |

These are the experiment's transition labels, not CA's generic verdicts or an
adequacy score. M and C use different control configurations; their counts are
not a ranking. A guard refusal does not mean the faulty selection was correct.

### Reuse CA's existing inspection commands

For an actual CA manifest or two `corpus-adequacy.report.v0` files, the existing
tool provides nonexecuting views:

```sh
python3 /path/to/corpus-adequacy/corpus_adequacy.py --inspect manifest.json
python3 /path/to/corpus-adequacy/corpus_adequacy.py --diff old.report.v0.json new.report.v0.json
```

`--inspect` checks declarations, not resource availability or runtime behavior.
`--diff` compares reported facts, not causes. The adapter package's `result.json`
and execution-observation finals are different formats: use its recount command,
not `--diff`. No new CA classification or format is introduced here.

### Before a separately authorized new trial

Reuse the original trial's resource and lifecycle checks; this package is not
a new executor. A short readiness record should name:

1. Exact source/input pins and the successful small regression command above.
2. Actual free disk space on every write destination, the complete write/capture
   reserve, and the remaining floor required by that trial's contract. Free
   space alone is insufficient, and a source-byte total is not filesystem usage.
3. The existing rehearsal's output-limit, cancellation and child-collection
   observations for the chosen host. A macOS file-size limit is not assumed to
   constrain every write mode.
4. Separate experiment, cleanup and outer-checker statuses. Preserve failed
   attempts; unknown or missing evidence remains unknown.

This is a preparation checklist, not an enforced resource gate or execution
permission. A changed host or execution contract needs fresh relevant evidence;
it does not require repeating an already completed trial merely to format its
results. Use the existing closeout procedure for retained-byte recount and
publication rather than adding another supervisor.

### Baseline and mutation verifier limits

Before decoding, the verifier bounds files to 4 MiB each, the tree to 64 MiB,
10,000 total entries, paths to 512 UTF-8 bytes, directory/JSON nesting to 64 and
JSON structural tokens to 250,000. File reads use no-follow directory descriptors
and compare scanned file identity before and after reading. JSON duplicate keys,
nonfinite numbers, invalid UTF-8 and trailing data refuse. These are input
ceilings; they are not an exact process-memory quota or an adversarial filesystem
snapshot guarantee. The pinned inventory additionally binds the accepted bytes.
