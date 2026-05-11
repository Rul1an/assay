# Release Process

This document outlines the canonical checklist for releasing new versions of Assay.

## Checklist

### 1. Preparation
- [ ] **Bump Versions**: Update `version` in `Cargo.toml` for all crates.
  - Root `Cargo.toml` (workspace members inheritance)
  - `crates/assay-common/Cargo.toml` (if not inherited)
  - `assay-python-sdk/Cargo.toml`
- [ ] **Update Lockfile**: Run `cargo check --workspace` to update `Cargo.lock`.
- [ ] **Changelog**: Update `CHANGELOG.md` with new features and fixes.
- [ ] **Lints**: Run `cargo clippy --workspace --all-targets` to ensure no new warnings.

### 2. Permissions Check (Crucial)
- [ ] **Trusted Publishing**: Ensure GitHub Actions OIDC is enabled for the release tag on every current crates.io crate:
  - `assay-common`
  - `assay-registry`
  - `assay-evidence`
  - `assay-core`
  - `assay-metrics`
  - `assay-policy`
  - `assay-mcp-server`
  - `assay-monitor`
  - `assay-sim`
  - `assay-cli`
- [ ] **Non-crates.io workspace members**: Confirm these remain `publish = false` unless a dedicated distribution freeze changes the contract:
  - `assay-adapter-api` (historical crates.io versions exist, but the current release line does not publish it)
  - `assay-adapter-acp`
  - `assay-adapter-a2a`
  - `assay-adapter-ucp`
  - `assay-it` (distributed through PyPI wheels, not crates.io)
  - `assay-ebpf`
  - `assay-xtask`
- [ ] **Public Crate Policy Check**: Run `bash scripts/ci/check-public-crate-policy.sh`.
- [ ] **Token Scopes**: If using a token fallback, ensure it has `publish-update` scope.

### 3. Execution
- [ ] **Tag**: Create and push the git tag.
  ```bash
  git tag vX.Y.Z
  git push origin vX.Y.Z
  ```
- [ ] **Watch CI**: Monitor the `release.yml` workflow.
  - Step: `Publish to Crates.io` (uses `scripts/ci/publish_idempotent.sh`).
  - Step: `Create GitHub Release` (upload binaries and release assets).
  - Step: `Build assay-mcp-server MCPB` (produces `release/assay-mcp-server-${VERSION}-linux.mcpb` plus `.sha256`).
  - Step: `Render generated registry metadata` (produces `release/server.json` for later MCP registry submission).
  - Step: `Generate CycloneDX SBOM bundle` (produces `release/assay-${VERSION}-sbom-cyclonedx.tar.gz` plus `.sha256`).
  - Step: `Enforce release attestation policy` (produces `release/assay-${VERSION}-release-provenance.json` plus `.sha256` and uploads raw attestation verification evidence as a workflow artifact).
  - Step: `Build release proof kit` (produces `release/assay-${VERSION}-release-proof-kit.tar.gz` plus `.sha256`).
  - Step: `Check release asset preflight` (fails before publication unless the `release/` directory exactly matches the expected asset contract, every `.sha256` verifies, and `server.json` points at the generated MCPB checksum).
  - Step: `Create GitHub Release` (uploads only the preflighted files from `release/`).

### 4. Verification
- [ ] **Install Check**: `cargo install assay-cli --version X.Y.Z`
- [ ] **LSM Smoke Test**: Manually dispatch the `lsm-smoke-test` workflow or run `scripts/verify_lsm_docker.sh --release-tag vX.Y.Z`.
- [ ] **SBOM Asset Check**: Confirm the GitHub release includes `assay-${VERSION}-sbom-cyclonedx.tar.gz` and `assay-${VERSION}-sbom-cyclonedx.tar.gz.sha256`.
- [ ] **MCPB Asset Check**: Confirm the GitHub release includes `assay-mcp-server-${VERSION}-linux.mcpb` and `assay-mcp-server-${VERSION}-linux.mcpb.sha256`.
- [ ] **Registry Metadata Check**: Confirm the GitHub release includes `server.json` generated from the MCPB asset and matching SHA-256.
- [ ] **Provenance Asset Check**: Confirm the GitHub release includes `assay-${VERSION}-release-provenance.json` and `assay-${VERSION}-release-provenance.json.sha256`.
- [ ] **Proof Kit Asset Check**: Confirm the GitHub release includes `assay-${VERSION}-release-proof-kit.tar.gz` and `assay-${VERSION}-release-proof-kit.tar.gz.sha256`.
- [ ] **Release Asset Preflight Check**: Confirm `Check release asset preflight` passed before `Create GitHub Release`; this is the machine-readable asset contract for GitHub release publication.
- [ ] **Workflow Evidence Check**: Confirm the workflow artifacts include `release-provenance-evidence` with the raw `gh attestation verify --format json` results for each release archive.
- [ ] **Offline Verification Check**: Unpack the proof kit and run `verify-offline.sh --assets-dir /path/to/release-assets` against the downloaded release archives. See [Release Proof Kit](../security/RELEASE-PROOF-KIT.md).
- [ ] **Operator Flow Check**: For the compact end-to-end story that connects transcript ingest, shipped `C2` pack evaluation, and proof-kit verification, see [Operator Proof Flow](../guides/operator-proof-flow.md).
- [ ] **Registry Publication Decision**: Treat `release/server.json` as publish-ready input, not proof of an existing live official registry listing.

## Troubleshooting

### HTTP 403 Forbidden
*   **Cause**: Missing ownership or Trusted Publishing not configured for a specific crate.
*   **Fix**: Go to crates.io settings for the failing crate and add the GitHub repository as a Trusted Publisher.

### Token not valid for crate
*   **Cause**: A crate in the current public release contract is missing a Trusted Publishing grant.
*   **Fix**: Configure crates.io Trusted Publishing for that crate. The release intentionally fails instead of silently skipping a public crate and creating release drift.

### "Crate already uploaded"
*   **Cause**: Partial failure in a previous run.
*   **Fix**: `publish_idempotent.sh` handles this automatically. Re-running the job is safe.
