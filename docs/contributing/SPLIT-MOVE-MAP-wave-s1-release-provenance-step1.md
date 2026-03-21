# Wave S1 Step1 move map: release provenance hardening

Single-source ownership:
- Release attestation verification policy -> `scripts/ci/release_attestation_enforce.sh`
- Workflow orchestration / asset publishing -> `.github/workflows/release.yml`
- Release operator contract -> `docs/reference/release.md`

Containment intent:
- Keep `gh attestation verify` flag policy out of inline workflow bash.
- Keep provenance JSON normalization in the helper script only.
- Keep shell contract coverage in `test-release-attestation-enforce.sh`.
