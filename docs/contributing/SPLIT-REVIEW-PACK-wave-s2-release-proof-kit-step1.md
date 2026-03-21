# Wave S2 Review Pack

## Primary Claim

Assay releases now ship a proof kit that allows offline provenance verification
with the same policy already enforced in CI.

## Review Focus

1. `S2` reuses the exact `S1` asset set.
2. The proof-kit manifest does not invent a second policy.
3. Offline verification is the canonical path.
4. Missing bundle or trusted root prevents any tarball from being emitted.
5. Docs are explicit about trust boundaries and non-goals.

## Key Files

- `.github/workflows/release.yml`
- `scripts/ci/release_archive_inventory.sh`
- `scripts/ci/release_attestation_enforce.sh`
- `scripts/ci/release_proof_kit_build.sh`
- `scripts/ci/test-release-proof-kit-build.sh`
- `docs/reference/release.md`
- `docs/security/RELEASE-PROOF-KIT.md`
- `scripts/ci/review-wave-s2-release-proof-kit-step1.sh`
