# Wave S1 Step1 checklist: release provenance hardening

Scope freeze:
- [ ] Release provenance policy only; no broad workflow reshaping.
- [ ] Existing attestation producer remains intact.
- [ ] Public change is limited to a new release provenance asset plus internal evidence artifact.

Files:
- [ ] `.github/workflows/release.yml`
- [ ] `scripts/ci/release_attestation_enforce.sh`
- [ ] `scripts/ci/test-release-attestation-enforce.sh`
- [ ] `scripts/ci/review-wave-s1-release-provenance-step1.sh`
- [ ] `docs/reference/release.md`
- [ ] `docs/contributing/SPLIT-*wave-s1-release-provenance-step1.md`

Policy anchors:
- [ ] verifier binds to `--repo`, `--signer-workflow`, and `--cert-oidc-issuer`
- [ ] verifier binds to `--source-digest` and `--source-ref`
- [ ] verifier denies self-hosted runners
- [ ] verified attestations must contain witness data (`verifiedTimestamps`)
- [ ] verified subjects must match local asset digests
- [ ] public summary asset records the verification policy and per-asset results
- [ ] raw verification artifact stays available for later audit/offline review
- [ ] wrong signer / workflow / subject mismatch all fail closed

Outputs:
- [ ] `release/assay-${VERSION}-release-provenance.json`
- [ ] `release/assay-${VERSION}-release-provenance.json.sha256`
- [ ] workflow artifact upload for raw provenance evidence

Validation:
- [ ] `bash -n` on touched shell scripts
- [ ] `bash scripts/ci/test-release-attestation-enforce.sh`
- [ ] workflow YAML parses cleanly
- [ ] reviewer script passes against `origin/main`
- [ ] `git diff --check`
