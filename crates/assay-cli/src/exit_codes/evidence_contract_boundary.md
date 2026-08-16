An evidence bundle was opened and read, and the bytes violate the bundle's declared format
contract: a typed `assay_evidence::ErrorClass::Contract` finding whose `ErrorCode` is one of the
`Contract*` variants. Establishes that the bundle does not conform to its own contract, and
nothing about recorded-value disagreement, how the defect was introduced, tampering, or intent.

An emitter MUST key on `ErrorClass::Contract` together with a `Contract*` code —
`ContractMissingManifest`, `ContractSchemaVersion`, `ContractFileOrder`, `ContractMissingFile`,
`ContractDuplicateFile`, `ContractUnexpectedFile`, `ContractRunIdMismatch`,
`ContractBundleIdMismatch`, `ContractSequenceGap`, `ContractSequenceStart`,
`ContractTimestampRegression`, `ContractInvalidJson`, `ContractInvalidEvent` — and MUST NOT map
a `Contract*` code under any other class. Those thirteen are exhaustive of the `Contract*`
prefix in `assay_evidence::ErrorCode`. A new `Contract*` variant is a new mapping decision, not
an automatic member.

Recorded-value mismatches (`IntegrityManifestHash`, `IntegrityEventHash`,
`IntegrityFileSizeMismatch`, `IntegrityRunRootMismatch`) remain `E_EVIDENCE_INTEGRITY`. Open or
archive-read failures (`IntegrityIo`, `IntegrityGzip`, `IntegrityTar`) remain
`E_EVIDENCE_UNREADABLE`. `Limits` and `Security` findings, and a well-formed bundle that fails a
profile's cardinality, vocabulary, or binding rules, are **outside** this code: they establish
different facts, and folding them in would make this code a whole-artifact or profile verdict.

Remediation is prose rather than a command, because re-verifying the same bundle only repeats
the same contract failure; conforming evidence has to come from the producer. Carries no
verdict: no test ran.
