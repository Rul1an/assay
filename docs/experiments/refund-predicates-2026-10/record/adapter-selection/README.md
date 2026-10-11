# Four report selections and their recorded mutation responses

This package contains locally recorded, known-answer synthetic fixture results
and selected source bytes from an adapter experiment. It is an export candidate,
not yet a released artifact. Federation-port v0 checks before dispatch: earlier
action evidence may support a later admission. This does not check the newly
dispatched action, validate the v1 target proposal, or establish executor behavior.

## Recount retained outcomes

From this directory, with Python 3:

```sh
python3 -I -S -B independent_reproducer.py .
```

Expected: 294 recorded invocations, 19 recorded application entries, 196 outcome
comparison cells and four separately bound reports. The checker imports no
producer code. It verifies receipt/stdout/final-reference bindings and recomputes
classification transitions. It compares declared commitments; it does not derive
those commitments from the original fixture semantics.

| Report | Projected status | Native reason |
| --- | --- | --- |
| LATE / r / 3 | not_established | unsupported_definite_report |
| LATE / r2 / 5 | established | terminal_support |
| LATE-CONFLICT / r / 4 | established | terminal_support |
| LATE-CONFLICT / r2 / 7 | not_established | contrary_terminal_evidence |

Missing selector S1 is refused. Duplicate report identity S3 is also refused,
with the uniqueness guard's `selection:subject_not_found` reason.

## What changes under the selection faults

The original literal replacements are in `source/mutation-recipes.json`.
The six declared implementation files and fourteen vectors are in `source/subject`.
`source/production/step-source-pins.json` records expected per-file hashes.
Applying each literal replacement offline matched all six recorded file pins
for all 21 source rows (two baseline rows and nineteen mutation/control rows).
`source-derivation.json` records that separate static comparison.

| Fault | With existing guards | With postcondition Q disabled in the separate S2 baseline |
| --- | --- | --- |
| First report (M1 / C5) | V2, V4 become projector refusals | V2, V4 change claim status/reason |
| Last report (M2 / C6) | V1, V3 become projector refusals | V1, V3 change claim status/reason |
| First established report (M3 / C7) | V1, V4 become projector refusals | V1, V4 change claim status/reason |

C5/C6/C7 also remove the selector's sequence guard; they are deliberate combined
faults, not identical single mutations to M1/M2/M3. The S1 combined versions
M5/M6/M7 are refused by postcondition Q on those same report pairs. S2 uses the
explicit A0 front variant with Q disabled; this is a controlled experiment,
not the normal adapter configuration.

Removing only the sequence check (M4) leaves V1–V4 unchanged. S4a changes refusal
and S4b changes refusal to a claim. The four named reports alone therefore do
not establish coverage of every selection fault.

The matrix notation: U unchanged; U* only trailing diagnostic difference;
SM claim status/reason change; AI refusal becomes a claim; RO refusal changes;
GR-P/GR-A claim becomes a projector/postcondition refusal. These are transitions,
not a product score or unconditional adequacy verdict. Full matrices remain in
`files/mount/run/result.json`.

## Evidence and limits

The 370 original recount inputs are unchanged; their inventory is MANIFEST.json.
The added 294 original dispatch records bind by digest to their receipts.
Selected source files are unchanged copies. `source/mutation-recipes.json` is
explicitly derived from two original manifests: only mutation recipes and declared
implementation paths are included; host command paths are omitted. It is not a
byte-identical copy of either original manifest. The static derivation does not
run any component or execute a mutation through CA.

The finals and result retain execution, cleanup and closure summaries. Supporting
host observations are omitted; this recount does not validate those assertions.
The original outer wrapper exited 1 on a final close-count assertion after RUN
exited 0. A later narrow checker correction did not rewrite that failure or rerun
CA. This package does not establish cleanup, native file-size containment, host
authenticity, complete effective imports or an independently witnessed execution.
Hashes and invocation/session identifiers are retained intentionally for joins.

This is source inspection plus a recount of recorded results, not a portable
runtime or complete experiment replay. Recorded source identities do not prove
what an untrusted host executed. No third-party adoption, blind evaluation,
production behavior or general conformance is claimed.

Five original source comments name `CLAUDE-ADAPTER-DESIGN-v6`. That filename is
retained intentionally to preserve exact source bytes. The underlying private
design document is not included; the name establishes neither review approval
nor reviewer identity.
