An archive member path in the evidence bundle was refused as unsafe to extract: a typed
`assay_evidence::ErrorClass::Security` finding whose `ErrorCode` is one of the `Security*`
variants. Establishes that a recorded path was absolute or could resolve outside the extraction
root, and nothing about the bundle's content, how the path came to be recorded, the producer's
intent, or whether anyone attempted an attack.

An emitter MUST key on `ErrorClass::Security` together with a `Security*` code —
`SecurityPathTraversal`, `SecurityAbsolutePath` — and MUST NOT map a `Security*` code under any
other class. Those two are exhaustive of the `Security*` prefix in `assay_evidence::ErrorCode`. A
new `Security*` variant is a new mapping decision, not an automatic member: a future variant that
records some other unsafe fact does not join this code by prefix.

Recorded-value mismatches remain `E_EVIDENCE_INTEGRITY`, format-contract defects remain
`E_EVIDENCE_CONTRACT`, open or archive-read failures remain `E_EVIDENCE_UNREADABLE`, and a
configured ceiling refusal is `E_EVIDENCE_LIMIT_EXCEEDED`. A refused path establishes a different
fact from all four and is not evidence that any of them also holds.

Remediation is prose rather than a command, because re-reading the same archive repeats the same
refusal and nothing this side of the producer makes the recorded path safe. Carries no verdict:
no test ran.
