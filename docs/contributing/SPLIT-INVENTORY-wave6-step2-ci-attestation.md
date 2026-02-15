# Wave6 Step2 inventory: attestation producer + verify pair

Snapshot baseline (`origin/main` before Step2): `1a5af4dd`
Working branch head: see `git rev-parse --short HEAD`

Target files:
- `.github/workflows/release.yml`
- `scripts/ci/review-wave6-step2-ci.sh`
- `docs/contributing/SPLIT-*wave6-step2-ci-attestation.md`
- `docs/architecture/PLAN-split-refactor-2026q1.md`

Step2 contract:
- Add provenance attestation producer in release workflow.
- Add attestation verification in release workflow with fail-closed behavior.
- Keep Wave0 gates and existing release behavior otherwise unchanged.

Planned producer/verify anchors:
- `uses: actions/attest-build-provenance@v2`
- `subject-path: release/*`
- `gh attestation verify ... --repo ... --signer-workflow ... --cert-oidc-issuer ...`
- fail-closed branch when no release archives are present.

Non-goals in Step2:
- no nightly fuzz/model lane changes (Wave6 Step3)
- no unrelated workflow refactors
