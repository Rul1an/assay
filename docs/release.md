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
- [ ] **Trusted Publishing**: Ensure GitHub Actions OIDC is enabled for the new version tag.
- [ ] **Crate Ownership**: Verify `crates.io` ownership for *all* workspace members:
  - `assay-core`
  - `assay-cli`
  - `assay-common`
  - `assay-monitor`
  - `assay-ebpf` (if published separately)
  - `assay-mcp-server`
  - `assay-metrics`
  - `assay-policy`
  - `assay-xtask`
- [ ] **Token Scopes**: If using a token fallback, ensure it has `publish-update` scope.

### 3. Execution
- [ ] **Tag**: Create and push the git tag.
  ```bash
  git tag v2.2.3
  git push origin v2.2.3
  ```
- [ ] **Watch CI**: Monitor the `release.yml` workflow.
  - Step: `Publish to Crates.io` (uses `scripts/ci/publish_idempotent.sh`).
  - Step: `Create GitHub Release` (upload binaries).

### 4. Verification
- [ ] **Install Check**: `cargo install assay-cli --version 2.2.3`
- [ ] **LSM Smoke Test**: Manually dispatch the `lsm-smoke-test` workflow or run `scripts/verify_lsm_docker.sh --release-tag v2.2.3`.

## Troubleshooting

### HTTP 403 Forbidden
*   **Cause**: Missing ownership or Trusted Publishing not configured for a specific crate.
*   **Fix**: Go to crates.io settings for the failing crate and add the GitHub repository as a Trusted Publisher.

### "Crate already uploaded"
*   **Cause**: Partial failure in a previous run.
*   **Fix**: `publish_idempotent.sh` handles this automatically. Re-running the job is safe.
