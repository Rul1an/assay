# Wave6 Step2 checklist: attestation pair

Scope lock:
- release workflow attestation producer + verify pair only
- docs/reviewer gates updates only
- no crate code changes

Artifacts:
- `docs/contributing/SPLIT-INVENTORY-wave6-step2-ci-attestation.md`
- `docs/contributing/SPLIT-CHECKLIST-wave6-step2-ci-attestation.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave6-step2-ci-attestation.md`
- `scripts/ci/review-wave6-step2-ci.sh`

Runbook:
```bash
BASE_REF=origin/main bash scripts/ci/review-wave6-step2-ci.sh
```

Hard gates:
- release job includes `attestations: write` and `id-token: write`.
- producer present: `actions/attest-build-provenance@v2` with `subject-path: release/*`.
- verifier present: `gh attestation verify` with signer workflow and OIDC issuer constraints.
- fail-closed path present for missing release assets and verification failures.
- strict diff allowlist.

Definition of done:
- reviewer script passes.
- release workflow contains producer + verify pair.
