# C2 baseline verification preparation

No retained C2 baseline is accepted by this revision. The command below always
returns exit 2, writes a `refused: pending` line to stderr and leaves stdout empty:

```sh
python3 verify_baseline.py --retained PATH_TO_RETAINED_DIRECTORY
```

The three acceptance hashes in the module are deliberately `None`. There is no
CLI option to supply replacement pins or bypass the pending gate. Passing the
synthetic tests does not establish that a C2 measurement happened.

## Implemented checks

The internal checks operate on bytes through the shared bounded `evidence_io`
loader. They do not import the instrument or reader, execute a subject, or follow
historical paths in provenance. The retained directory is limited to regular
files; archives, symlinks, traversal, duplicate JSON keys and excessive byte,
entry, nesting or token counts refuse before unrestricted materialization.

- Frozen reader, oracle and fixture manifest hashes; all 20 packets and 25
  operation rows. Retained subject, vector and fixture pin checks are prepared.
- Full raw report equality to the frozen packet identity, literal oracle and
  non-claims. All four ordered root selectors and every operation field remain
  in the comparison. Boolean/integer substitutions do not compare equal.
- Slot, raw receipt, stream, dispatch and invocation identities; durable intent
  binding, exact journal coverage and session directory identity. Duplicate,
  missing or extra journal ordinals refuse.
- Stopped baseline prefix, an empty completed build, declined positive control,
  ordinary step not run, cleanup evidence and a final document differing only
  by its schema and exact prefix hash. No admission or consumption is allowed.
- Complete byte inventory and mandatory result/provenance pair. The provenance
  must bind the result; failed attempt state, pending result and persistence
  failure markers refuse even when that hash relationship is valid. The
  deterministic result is compared separately with the full raw reports.

These are preparation functions, not an integrated acceptance path. The small
synthetic tests exercise their report, invocation, journal, stop/close, inventory
and pair behavior. The source/vector integration function has not been exercised
against a retained C2 run. Complete package orchestration, instrument/context and
provenance cross-bindings, resource-account reconciliation, final pinning and the
independent result review must be completed against the actual retained bytes.
No end-to-end C2 verification is claimed.

## Format provenance and pending work

The success format was inspected at carrier source
`48bff8f8dca77a58304954f58f6127595b4a915f`; repaired finalization was inspected at
`1b5f9b42694a6b0bd64f624c20eebd2c4a154cfe`. These are format references, not proof
of execution or approval. Finalization was still under review when this
preparation was written. Its final disposition must be reconciled before
acceptance is enabled. Native serialization and observation structure follow the
public instrument codec at `7f4c8785fedbe43cfceb1d3e8cb26c7028215d08`.
No prior reviewer implementation was read or copied.

The initial absent-module RED was a setup failure. A subsequent behavioral RED
showed acceptance of journals relocated to a different session directory; the
check now refuses that case. Tests use synthetic format bytes and existing
frozen packet bytes, with no instrument execution or baseline measurement.

Run only the named top-level modules from this directory:

```sh
python3 -m unittest -v test_verify_baseline test_evidence_io test_verify_mutation
```

The previously retained C3 result and its verification remain separate. No
baseline, mutation score, provider behavior, runtime coverage, source custody or
reviewer authentication is inferred from this preparation. Actual C2 input
integration, independent technical review and privacy/publication review remain
separate gates.
