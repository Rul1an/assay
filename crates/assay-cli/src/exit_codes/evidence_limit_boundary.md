Evidence verification stopped at a configured resource ceiling: a typed
`assay_evidence::ErrorClass::Limits` finding whose `ErrorCode` is one of the `Limit*` variants.
Establishes that inspection refused to continue, and nothing about the bundle's content: the
bundle may be entirely valid, and no content, contract, or profile verdict was reached. This is a
refusal to look, not a finding about what was looked at.

An emitter MUST key on `ErrorClass::Limits` together with a `Limit*` code — `LimitBundleBytes`,
`LimitDecodeBytes`, `LimitFileSize`, `LimitLineBytes`, `LimitTotalEvents`, `LimitPathLength`,
`LimitJsonDepth` — and MUST NOT map a `Limit*` code under any other class. Those seven are
exhaustive of the `Limit*` prefix in `assay_evidence::ErrorCode`. A new `Limit*` variant is a new
mapping decision, not an automatic member.

Recorded-value mismatches remain `E_EVIDENCE_INTEGRITY`, format-contract defects remain
`E_EVIDENCE_CONTRACT`, open or archive-read failures remain `E_EVIDENCE_UNREADABLE`, and an
unsafe archive member path is `E_EVIDENCE_PATH_REJECTED`. Folding a ceiling refusal into any of
them would report a fact about content that nobody measured.

Remediation is prose rather than a command, because no invocation of this tool raises the
ceiling: a smaller bundle has to come from its producer, or the operator configures a higher
limit deliberately. Carries no verdict: no test ran.
