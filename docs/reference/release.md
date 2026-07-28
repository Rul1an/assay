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
- [ ] **Release parser toolchain**: Use Ruby `3.3.12` with Psych `5.1.2`.
  GitHub Actions installs the pinned Ruby before running the release-channel
  contract; the local version-line preflight fails closed on another parser
  toolchain so YAML key semantics cannot drift between operator and CI.
- [ ] **Version-line preflight**: Bind the workspace to the intended stable tag before it exists:
  ```bash
  EXPECTED_RELEASE=vX.Y.Z CHECK_VM=0 \
    bash scripts/ci/check-assay-version-line.sh
  ```
  This is the workspace-only pre-tag check. On the host that owns the runner
  VM, repeat it with `CHECK_VM=1` to prove the VM still matches GitHub Latest
  while the workspace matches the intended release target.
  The Harness default is an independently proven compatibility pin and may
  intentionally lag the latest Assay release. The VM remains bound to the
  current GitHub Latest release until the new tag is published.

### 2. Permissions Check (Crucial)
- [ ] **Trusted Publishing**: Ensure GitHub Actions OIDC is enabled for the release tag on every current crates.io crate:
  - `assay-common`
  - `assay-registry`
  - `assay-canonical`
  - `assay-evidence`
  - `assay-core`
  - `assay-metrics`
  - `assay-policy`
  - `assay-mcp-server`
  - `assay-monitor`
  - `assay-runner-schema`
  - `assay-runner-linux`
  - `assay-runner-core`
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
  - `gateway-evidence-replay`
- [ ] **Public Crate Policy Check**: Run `bash scripts/ci/check-public-crate-policy.sh`.
- [ ] **Public MSRV Check**: Run
  `ASSAY_PUBLIC_MSRV=1.89.0 scripts/ci/check-msrv-policy.sh`.
- [ ] **Token Scopes**: If using a token fallback, ensure it has `publish-update` scope.

### 3. Execution
- [ ] **Tag**: Create and push the git tag.
  ```bash
  git tag -a vX.Y.Z -m "Assay vX.Y.Z"
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
- [ ] **Published MSRV Install Check**: use a fresh install root so Cargo cannot reuse an existing
  installation, then execute the resulting binary:
  ```bash
  install_root="$(mktemp -d)"
  trap 'rm -rf "$install_root"' EXIT
  rustup run 1.89.0 cargo install assay-cli \
    --locked --version X.Y.Z --root "$install_root"
  "$install_root/bin/assay" --version
  ```
  This exercises the lockfile shipped with the published CLI rather than the workspace lock.
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
