# Interop record: SEP-2828 `decision_pairing_v0`

Assay verifies MCP server-side execution records as an independent consumer. This is the record of
running that verifier against the upstream conformance vectors, so the claim is checkable rather
than asserted.

**Result: 6 of the 7 normative cases reproduced. 1 not reproduced, for a reason documented below.**

Re-run it yourself:

```bash
cargo build -p assay-cli --release
scripts/interop/reproduce-sep2828-decision-pairing.sh
```

The vectors are published by [vaaraio/vaara](https://github.com/vaaraio/vaara) under
AGPL-3.0-or-later. The script fetches them at run time and never vendors them into this MIT
repository. No upstream code is fetched or executed. Assay computes the individual checks from
the published JSON wire bytes; the local
fallback classifier applies this record's disposition rule to those check results.

## Why this record exists

Assay's position is that undeclared coverage reads as coverage, so a verifier should publish both
what it checked and what it did not. That applies to Assay first. "Assay reproduces these vectors"
is a claim about this project's own software, and a producer asserting its own conformance is
exactly the shape Assay argues is insufficient. Hence a re-runnable script and a written scope,
rather than a sentence in a README.

## What was reproduced

| Case | Expected | Assay |
|---|---|---|
| `valid_pair_allow_executed` | paired | Check A and Check B both hold |
| `decision_only_escalate` | no outcome required | decision-only report, no pairing asserted |
| `substituted_attestation_backlink` | Check A fails | outcome attestation digest mismatch, pairing refused |
| `substituted_pairing_nonce` | Check A fails | attestation digest holds, nonce mismatch, pairing refused |
| `substituted_decision_under_shared_attestation` | Check A holds, Check B fails | back-links match, `outcomeDerived.decisionDigest` does not |
| `supersession_equal_decidedat_tie` | `ambiguous` | `ambiguous`, reason `supersession_ambiguous_missing_sequence` |
| `fallback_envelope_binding` | binding holds | named projection is present; both envelope-digest checks fail, see below |

Two of these are worth calling out as good corpus design. `substituted_pairing_nonce` and
`substituted_decision_under_shared_attestation` are the cases that separate an implementation which
actually distinguishes instance binding from content binding from one that merely compares
back-links. A corpus that only carried the happy path and a single tampered record would not tell
those apart.

Supersession is a separate command, `assay evidence verify-mcp-supersession`, because it reasons
over a set of decisions sharing one back-link rather than over a decision and outcome pair.

## What Assay does not check

Declared rather than left to inference:

- **Signatures and issuer trust.** Assay verifies no signatures and resolves no keys, so
  `decision_signature_ok` and `receipt_signature_ok` are neither confirmed nor contradicted here.
  Reported as `signature_verification` and `issuer_key_trust` in `claims_not_made`.
- **Result payload binding.** The result commitment's own integrity is checked, since an
  `ArgsProjection` commits `projectionDigest` over the bytes of its `projection` string. Whether
  the committed value is what the tool returned needs the runtime result, which a record consumer
  does not hold. Reported as `result_commitment_payload_binding`.
- **`ArgsRef` dereferencing.** Only the recomputable half of a commitment is verified. An
  `ArgsRef` addresses content by URI, and this verifier fetches nothing, so its `digest` is
  reported as `ref_digest` and never checked against anything. Reported as
  `result_commitment_ref_not_dereferenced`. The embedded digest of the hash-only-identity
  `ArgsProjection` form is surfaced on the same terms: shown, not checked.
- **Runtime effect.** A reproduced verdict says the records bind to each other as specified. It
  does not say the recorded action occurred. Reported as `runtime_side_effect_truth`.

## The case that was not reproduced

`fallback_envelope_binding` is the no-attestation path, where the back-link digest is taken over a
named, versioned projection of the request envelope rather than over an attestation.

Both implementations do the versioning right. Each names its projection, and each binds that name
into the digest pre-image, so a mismatch between two different projections is visible rather than
silent. Assay computes `assay.fallback_projection.v0`; the vectors carry
`tools_call_params_plus_meta_authorization_binding_v1`.

Assay does not implement the upstream projection version, and reconstructing it from the published
specification text turned out not to be possible. The current vector does publish the request
envelope, decision, and receipt, so this record now executes the case directly. Assay confirms the
binding block, request nonce, decision/outcome backlink, outcome decision digest, and result
commitment. It refuses the pair only on the two request-envelope digest checks. The measured
Assay projection is `assay.fallback_projection.v0`; the decision names
`tools_call_params_plus_meta_authorization_binding_v1`. Those identifiers denote different object
shapes and therefore different digest pre-images.

That is a general property rather than a complaint about one profile, and it is raised as such in
the venue where the rule is being written, on the SCITT Canonical Payload Binding draft:
[action-state-group/scitt-payload-binding#5](https://github.com/action-state-group/scitt-payload-binding/issues/5).
Short version: an identifier that travels is necessary but not sufficient. It has to resolve to a
published pre-image construction, or it names a shape only its author can build.

The practical consequence is worth stating because it is the part that bites. A pre-image built
from a good-faith reading of the specification produces a digest mismatch, which is the same signal
the binding raises for a tampered record. A careful second implementer therefore concludes the
artifact is bad rather than that their pre-image is wrong.

## Provenance

| | |
|---|---|
| Run | 2026-08-23 |
| Assay | `assay-cli` 5.4.0, release profile |
| Verifier commands | `assay evidence verify-mcp-records`, `assay evidence verify-mcp-supersession` |
| Upstream vectors | Vaara `v1.75.0`, `tests/vectors/decision_pairing_v0/normative` at `9fefe51a61f16dc13cd64ca8ca4b8792e48fb64b` |
| Method | upstream JSON fetched and read; no upstream code fetched or executed; checks computed by `assay`, recorded fallback disposition applied locally |

The upstream revision is pinned rather than tracked from `main`, because a record whose inputs can
move is not a record. Drift shows up as a fetch failure instead of as a quietly different number.
`ASSAY_INTEROP_REV` re-runs the comparison against another commit. The script prints the revision
and the versions of `assay` and `python3` it used, and fails closed on a bad fetch or an
unexpected tool exit rather than reporting either as a comparison result. Every fetched JSON file
and the fallback report are bounded to 1 MiB before parsing.

The comparison is per check, not per case. Each row above pins the specific check ids that must
pass and must fail, so a case that reaches the right overall verdict for the wrong reason is
reported as a divergence. The fallback row also pins Assay's binding mode, projection identifier,
and digest source. A newly reproduced fallback is likewise reported as drift until this record is
updated; the script cannot silently retain the 6-of-7 result.
