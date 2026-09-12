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
- [ ] **Candidate source declaration**: Verify the checked-out candidate source and record its commit:
  ```bash
  CANDIDATE_TAG=vX.Y.Z EXPECTED_SHA="$(git rev-parse HEAD)" \
    bash scripts/ci/check-tag-tree-outward-truth.sh
  ```
  This binds the workspace, changelog, and generated golden-path source identity
  to the candidate tag, and verifies the caller-provided checkout SHA. It also
  checks that the README attestation row names the in-toto Statement and
  predicate versions the source emits. It does not
  prove that a not-yet-created tag already points at that commit. The published install pin may still name the
  previous release until the candidate assets exist; installability and source
  identity are separate checks.
  Published release tags are immutable and are never moved or rewritten; a bad
  published tag requires a new version.

### 2. Permissions Check (Crucial)
- [ ] **PyPI Trusted Publisher**: In the `assay-it` project Publishing page, require exactly one
  GitHub publisher: repository `Rul1an/assay`, workflow `release.yml`, environment `pypi`.
  Remove every other publisher, including the legacy `publish.yml` identity. An empty environment
  is broader authority and does not match this contract. Before creating a tag, run
  `python3 scripts/ci/check-release-runbook-truth.py`, compare its expected identity with every
  owner-visible PyPI publisher row, and retain a redacted receipt containing only the project,
  repository, workflow, environment, publisher count, observation time, and result.
- [ ] **Trusted Publishing**: Require, per crate, a GitHub Trusted Publisher:
  repository `Rul1an/assay`, workflow `release.yml`, environment `crates`.
  Remove every other publisher, including any publisher whose environment is unset.
  An unset environment is broader authority and does not match this contract. Before creating a tag, run
  `python3 scripts/ci/check-release-runbook-truth.py`, compare its expected identity with every
  owner-visible crates.io publisher row, and retain a redacted receipt containing only the crate,
  repository, workflow, environment, publisher count, observation time, and result.
  No credentials. Apply this on every current crates.io crate:
  - `assay-common`
  - `assay-registry`
  - `assay-canonical`
  - `assay-evidence`
  - `assay-adapter-api`
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
- [ ] **GHCR environment**: Create a GitHub Environment named `ghcr` and attach it to
  `publish-image` in `release.yml`. The job needs `packages: write`, `id-token: write`, and
  `attestations: write`. Required reviewers on that environment are an owner choice; the workflow
  already waits on `environment: ghcr` before it can push.
- [ ] **GHCR package visibility**: GitHub creates user packages private. After the first image
  exists, an owner must make `ghcr.io/rul1an/assay-mcp-server` public in the package settings.
  That change cannot be undone. `verify-published-image` pulls the digest anonymously, so a
  private package fails that job. The first real publication should be an rc tag (crates.io,
  PyPI, and the MCP registry already skip `-rc` / `-beta`); do not put a digest into the install
  docs until `verify-published-image` is green on a stable tag.

### 3. Execution
- [ ] **Tag**: Create and push the git tag.
  ```bash
  git tag -a vX.Y.Z -m "Assay vX.Y.Z"
  git push origin vX.Y.Z
  ```
- [ ] **Watch CI**: Monitor the `release.yml` workflow.
  The GitHub Release is created before crates publication because `publish-crates` needs `release`.
  - Step: `Build assay-mcp-server MCPB` (produces `release/assay-mcp-server-${VERSION}-linux.mcpb` plus `.sha256`).
  - Step: `Render generated registry metadata` (produces `release/server.json` for later MCP registry submission).
  - Step: `Generate CycloneDX SBOM bundle` (produces `release/assay-${VERSION}-sbom-cyclonedx.tar.gz` plus `.sha256`).
  - Step: `Enforce release attestation policy` (produces `release/assay-${VERSION}-release-provenance.json` plus `.sha256` and uploads raw attestation verification evidence as a workflow artifact).
  - Step: `Build release proof kit` (produces `release/assay-${VERSION}-release-proof-kit.tar.gz` plus `.sha256`).
  - Step: `Check release asset preflight` (fails before publication unless the `release/` directory exactly matches the expected asset contract, every `.sha256` verifies, and `server.json` points at the generated MCPB checksum).
  - Step: `Create GitHub Release` (uploads only the preflighted files from `release/`).
  - Job: `publish-image` (`Publish GHCR image`; needs `[release-contract, release]`; environment `ghcr`).
    Stages the sha256-verified `x86_64` / `aarch64-unknown-linux-gnu` `assay-mcp-server` binaries,
    copies them into `gcr.io/distroless/cc-debian13:nonroot` (no rebuild), pushes
    `ghcr.io/rul1an/assay-mcp-server:vX.Y.Z`, and attaches GitHub attestations (provenance +
    CycloneDX SBOM) with `push-to-registry: true` and `create-storage-record: false`. Stable tags
    also receive `X.Y` and `latest`; rc / beta tags do not.
  - Job: `verify-published-image` (`Verify published image`; needs `publish-image`).
    Pulls the digest anonymously on amd64 and arm64, runs `--version` as user `65532:65532`,
    byte-compares the image binary with the release tarball, and runs both `gh attestation verify`
    checks.
  - Job: `publish-crates` (`Publish to crates.io`; uses `scripts/ci/publish_idempotent.sh`).

### Published binary installability

`installer` means `scripts/install.sh` installs the component for that target.
`manual_step` means a release archive exists but the installer does not install
that component. `unsupported` means this release publishes no matching binary;
it is not an installer failure.

<!-- release-installability-matrix:start -->
| Component | Target | Install status | Release asset |
| --- | --- | --- | --- |
| `assay` | `x86_64-unknown-linux-gnu` | `installer` | `assay-v6.2.1-x86_64-unknown-linux-gnu.tar.gz` |
| `assay` | `aarch64-unknown-linux-gnu` | `installer` | `assay-v6.2.1-aarch64-unknown-linux-gnu.tar.gz` |
| `assay` | `x86_64-apple-darwin` | `installer` | `assay-v6.2.1-x86_64-apple-darwin.tar.gz` |
| `assay` | `aarch64-apple-darwin` | `installer` | `assay-v6.2.1-aarch64-apple-darwin.tar.gz` |
| `assay` | `x86_64-pc-windows-msvc` | `installer` | `assay-v6.2.1-x86_64-pc-windows-msvc.zip` |
| `assay-mcp-server` | `x86_64-unknown-linux-gnu` | `manual_step` | `assay-mcp-server-v6.2.1-x86_64-unknown-linux-gnu.tar.gz` |
| `assay-mcp-server` | `aarch64-unknown-linux-gnu` | `manual_step` | `assay-mcp-server-v6.2.1-aarch64-unknown-linux-gnu.tar.gz` |
| `assay-mcp-server` | `x86_64-apple-darwin` | `unsupported` | `-` |
| `assay-mcp-server` | `aarch64-apple-darwin` | `unsupported` | `-` |
| `assay-mcp-server` | `x86_64-pc-windows-msvc` | `unsupported` | `-` |
<!-- release-installability-matrix:end -->

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
- [ ] **Optional LSM verification**: Not a stable-release requirement. Dispatch `release.yml` with the optional `workflow_dispatch` input `verify_lsm`, or run the supported local path `scripts/verify_lsm_docker.sh --release-tag vX.Y.Z`.
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
