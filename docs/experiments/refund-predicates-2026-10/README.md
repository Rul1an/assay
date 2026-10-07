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

**Current local package status:** C2 has reader/reproducer examples; its separate
retained baseline is pending. No C2 baseline result is implied by passing the
focused tests. This package also carries one original C3 mutation run and a new
read-only verifier. Final package review and release are separate from local
verification.

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

## Verify the original C3 mutation run offline

```sh
python3 -B verify_mutation.py
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest -v test_evidence_io test_verify_mutation
```

`verify_mutation.py` reads all retained files and raw reports. It does **not**
execute retained source, imports from the retained instrument, or any original
command. Optional `--retained`, `--mutation-inputs` and `--c3-inputs` arguments
select local directories; the verifier still requires the exact pinned original
inventory and result. Archives are refused; provide a plain directory tree.
The public output is a bounded verification record, not a new measurement.
Missing evidence, malformed inputs, links, changed bytes or failed checks refuse
with exit 2 and no stdout. There is no partial-pass mode.

The original measurement used the public observation interface of
[Corpus Adequacy](https://github.com/Rul1an/corpus-adequacy) at
`7f4c8785fedbe43cfceb1d3e8cb26c7028215d08`. Its seven original exported source files
are preserved as inert evidence under `record/mutation/retained/run/export/`; the pinned
MIT license and copyright are in [record/mutation/CA-LICENSE](record/mutation/CA-LICENSE).
The verifier uses no CA API. This package grants no authority to resume sessions,
repeat the measurement, contact an external party or assert a result about an
external implementation. C1/C2/C3 focused-test results do not establish that CA
was used on those slices. The original C3 mutation run is the retained CA-use
claim supported here.

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

## Evidence identity and limits

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

The historical budget covers logical evidence bytes, excluding temporary subject
copies, CA lock files, RAM and filesystem allocation. The largest saved boundary
value is not a peak, and no free-space samples were retained. Unsampled transient
writes depend on the original reservation argument; this offline verifier does
not turn that into a disk trace.

## Input limits

Before decoding, the verifier bounds files to 4 MiB each, the tree to 64 MiB,
10,000 total entries, paths to 512 UTF-8 bytes, directory/JSON nesting to 64 and
JSON structural tokens to 250,000. File reads use no-follow directory descriptors
and compare scanned file identity before and after reading. JSON duplicate keys,
nonfinite numbers, invalid UTF-8 and trailing data refuse. These are input
ceilings; they are not an exact process-memory quota or an adversarial filesystem
snapshot guarantee. The pinned inventory additionally binds the accepted bytes.
