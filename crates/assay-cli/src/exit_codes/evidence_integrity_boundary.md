An evidence bundle was opened and content that was read failed verification: a member hash, an
event's `content_hash`, a manifest entry, or the run integrity chain (`run_root`) disagrees with
what the bundle records. Establishes that the content does not verify, and nothing about how it
came to differ, so it carries no tampering or intent claim.

`run_root` is a hash chain over the content hashes in sequence order, not a tree. Naming it for the
structure `assay-evidence` actually builds keeps this rule clear of the inclusion-proof and
sub-range properties a tree would imply to a reader.

A bundle that could not be opened or read is deliberately **outside** this code: an I/O failure
establishes no fact about content, and reporting one as an integrity finding would assert a fact
nobody measured. **`assay_evidence::ErrorClass::Integrity` is therefore not the mapping key**, and
neither is the `Integrity*` code prefix: `impl From<std::io::Error> for VerifyError` maps every I/O
failure to `Integrity`/`IntegrityIo`, and a read failure mid-stream is reported as `IntegrityGzip`
or `IntegrityTar`, so all three are indistinguishable from a content defect at that granularity.

An emitter MUST key on the four verifier codes that establish a recorded-value disagreement —
`IntegrityManifestHash`, `IntegrityEventHash`, `IntegrityFileSizeMismatch`,
`IntegrityRunRootMismatch` — and MUST NOT map `IntegrityIo`, `IntegrityGzip` or `IntegrityTar` to
this code until the verifier can separate the read from the content. Those four are exhaustive: the
opening sentence lists the kinds of thing that can disagree, not an open set of codes.

`Contract*`-class failures are **outside** this code as well, even though the bundle was opened and
read. They establish that the bundle violates its own format contract, which is a different fact
from a recorded value disagreeing with the bytes; folding the two together would make this code a
whole-artifact verdict. **Stated gap:** nothing is registered for a format-contract defect today,
so a consumer that meets one has no reason code to emit and MUST NOT stretch this one to fit.
`docs/experiments/evidence-mutation-cost-2026-06/results/matrix.md` measures the size of that gap:
of its 496 detections, 3 have a dominant verifier code this rule admits, 9 have one it forbids
(`IntegrityIo`), and 484 have one it does not name, 479 of them `ContractInvalidJson`. #2219 tracks
registering a code for them.

Remediation is prose rather than a command, because re-verifying the same bundle only repeats the
same failure; an undamaged bundle has to come from the producer. Carries no verdict: no test ran.
