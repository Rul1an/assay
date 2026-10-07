# C2 retained baseline verification

The former preparation gate is now connected to one original retained baseline.
It contains 20 successful process receipts and 25 operation rows. All full raw
reports match the frozen packet identities, literal oracle and non-claims.
The positive control was declined before execution; the ordinary mutation was
not run. Closing the stopped prefix supplied no admission. This baseline does
not measure fault discrimination or produce an adequacy score.

From this directory, with Python 3.11+ on a POSIX filesystem:

```sh
python3 -B verify_baseline.py
PYTHONDONTWRITEBYTECODE=1 python3 -B -m unittest -v test_verify_baseline test_evidence_io test_verify_mutation
```

The command reads the bundled `record/baseline-c2/` directory. `--retained PATH`
may select a relocated copy, but the exact inventory, result and provenance pins
remain mandatory. There is no caller-supplied pin or partial-pass option. Missing,
changed, malformed or inconsistent evidence gives exit 2, stderr and no stdout.
Success gives exit 0 and a bounded JSON verification record. No subject, original
carrier, instrument module or historical command is executed. Only the named
test modules above are discovered.

## Evidence and source identity

- Carrier source: `6bdeb4b10f3952ccd2fe6f792345f5a0d44efec4`.
- Inventory SHA256:
  `a3945cff972dede78a5002d8b0a25706d5d1f4ce243c13d2cfe7f03fda166cd3`.
- Result SHA256:
  `402cef44fbdb891f870414b7a0e5da14a7ca36422af02c5e7e538ad4cac31f6e`.
- Provenance SHA256:
  `766dc3696740535f08376194ee08d21e3c17b745b042ebbbefaec79dc8749d6a`.

All 228 original retained files, including the inventory, remain byte-identical.
`record/c2-carrier/source-map.json` binds six relevant carrier files to their
source paths, sizes, hashes and Git modes at the carrier revision. They are inert
source evidence; this package does not provide a command to rerun the carrier.
The seven instrument files in `record/baseline-c2/run/export/` match the external
source pin `7f4c8785fedbe43cfceb1d3e8cb26c7028215d08`. Their MIT license and copyright
are preserved in `record/mutation/CA-LICENSE`, covering both retained exports.

CA's own observation says `tool_commit: null` and
`tool_source_state: "unresolved"`: the original export was gitless. The verifier
preserves those values and recomputes its native content digest from all seven
files. External source-pin equality does not turn the native commit into a
resolved identity, authenticate custody, or prove the code objects loaded by the
historical process. The environment digest is retained as an opaque observation;
its underlying historical interpreter binary and environment are not reproduced.

## Verification scope

The verifier checks exact inventory coverage, source/fixture/vector pins,
context/provenance/source bindings, native instrument framing, intended schedule
and initial intent, both completed checkpoints, all 20 raw receipt chains, all
four ordered root selectors and every field of all 25 operation rows. It checks
both prefix and close copies of every required content-addressed blob and the
final document's binding to the original prefix. Extra files, missing journals,
reused identities, failed invocations and boolean/integer substitutions refuse.

The result and provenance are mandatory and bound together. Incomplete
finalization, failed attempt state, `result.pending.json` or
`persistence-failure.json` refuse even if a result/provenance hash pair matches.
The deterministic result is rederived from the verified raw reports. The
verifier imports neither original consumer nor instrument code; the author read
the carrier and public codec as format sources and did not use reviewer code.

## Resource evidence limits

The saved provenance contains 55 budget decisions and 142 historical free-space
samples. The verifier rederives the recorded write sizes, receipt-boundary
logical byte counts, prefix-copy allowance and final-artifact reservation from
the retained bytes and pinned carrier's serialization. Every saved sample must
meet the declared 5 GiB floor with its expected role and label.

The snapshot ends before the provenance write, exclusive publication and
retention. Its largest recorded `used` value, 724,695 bytes, is **not** an actual
peak. It contains no late free-space samples, so it does not prove free space
through those later operations. Historical paths, interpreter metadata and disk
samples are inert labels; no current availability or authority is implied, and
the verifier never resolves those paths.

The final original run tree has 775,840 logical file-content bytes. The retained
copy has 821,506 bytes including its 45,666-byte inventory. Their combined terminal
size is 1,597,346 bytes, below the declared 256 MiB logical budget. This arithmetic
counts the original run size represented by the inventory plus the packaged
copy; it does not inspect a current original directory. It is neither peak usage
nor filesystem allocation. The separate historical wrapper log, temporary
subject copies, locks, interpreter memory and filesystem metadata are excluded.

## Test and review boundaries

Preparation tests first exposed a journal-session-directory omission. Integration
began with the existing pending gate refusing the real pinned package. The
integration tests then exposed the native schedule's normalization of an omitted
ordinary `control` field to `false` before hashing; the verifier now applies that
pinned codec rule. Test-only tamper copies exercise wrong context, dispatch,
source/vector bytes, budget arithmetic/samples and finalization, including after
an inventory is rehashed. A relocated package verifies without original paths or
CA installation. None of these tests reruns a historical measurement.

The earlier C3 evidence and results remain separate. These synthetic declarations
do not establish a real refund, provider finality, exactly-once behavior, refund
safety, complete runtime coverage, an independent party's corroboration or a
second measured reproduction. Final technical review, privacy/publication review
and release approval remain separate from this offline check.
