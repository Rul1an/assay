# Wave S1 Step1 inventory: release provenance hardening

Snapshot baseline (`origin/main` before Step1): `054dc2ae`
Working branch head: see `git rev-parse --short HEAD`

Target files:
- `.github/workflows/release.yml`
- `scripts/ci/release_attestation_enforce.sh`
- `scripts/ci/test-release-attestation-enforce.sh`
- `scripts/ci/review-wave-s1-release-provenance-step1.sh`
- `docs/reference/release.md`
- `docs/contributing/SPLIT-*wave-s1-release-provenance-step1.md`

Step1 contract:
- Keep the existing GitHub attestation producer in the release flow.
- Move provenance verification policy into one script.
- Fail closed on:
  - no release archives
  - no verified attestations
  - missing verified timestamps / transparency witnesses
  - subject digest mismatch
- Export a public release provenance summary asset plus raw verification evidence.

Planned policy anchors:
- `gh attestation verify ... --source-digest ... --source-ref ... --deny-self-hosted-runners --format json`
- summary asset: `release/assay-${VERSION}-release-provenance.json`
- raw verification evidence under `artifacts/release-provenance/`

Non-goals in Step1:
- no Rekor/Fulcio integration
- no registry/resolver changes
- no release matrix refactor
- no dependency churn
