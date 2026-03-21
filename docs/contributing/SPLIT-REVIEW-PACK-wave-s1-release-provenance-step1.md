# Review Pack: Wave S1 Step1 (release provenance hardening)

Intent:
- harden the existing GitHub attestation release flow into a reusable, fail-closed provenance policy.

Scope:
- `.github/workflows/release.yml`
- `scripts/ci/release_attestation_enforce.sh`
- `scripts/ci/test-release-attestation-enforce.sh`
- `docs/reference/release.md`
- wave review artifacts + reviewer script

What changes:
- keep the attestation producer
- replace inline verification with a single-source script
- require source binding and timestamp/transparency witnesses
- export a public release provenance summary asset and raw workflow evidence

Acceptance:
- attestation verification is policy-bound to the expected repo/workflow/issuer
- subject digests match the concrete release assets
- public summary asset describes the verification contract and per-asset result set
- raw verification evidence remains available for later audit/offline inspection
- missing attestation, wrong signer/workflow, or subject mismatch fail closed

Reviewer command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave-s1-release-provenance-step1.sh
```

Expected result:
- PASS with allowlisted diff only.
