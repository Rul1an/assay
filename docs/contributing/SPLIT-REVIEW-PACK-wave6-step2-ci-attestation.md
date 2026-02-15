# Review Pack: Wave6 Step2 (attestation pair)

Intent:
- implement fail-closed attestation pair in release flow.

Scope:
- `.github/workflows/release.yml`
- docs + reviewer script for Step2

Producer/verify model:
- Producer: `actions/attest-build-provenance@v2` over `release/*`.
- Verify: `gh attestation verify` per release archive with signer workflow and OIDC issuer checks.
- Fail-closed behavior:
  - no release archive => fail
  - verification retry exhausted => fail

Reviewer command:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step2-ci.sh
```

Expected result:
- PASS with allowlisted diff only.
