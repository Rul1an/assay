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
  predicate versions the source emits. An internal dependency declaration must carry a
  version, and the tag guard refuses one without it. It does not
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
  - Step: `Attach build-provenance bundle` (copies `steps.attest-release.outputs.bundle-path` to `release/assay-${VERSION}-build-provenance.sigstore.json`; fails if the attest action did not expose a readable file).
  - Step: `Write and sign checksums.txt` (writes a name-sorted sha256 manifest over every other file in `release/`, keyless-signs it with cosign, and fails the job if the signature does not verify as the release workflow at `refs/tags/${VERSION}`).
  - Step: `Verify candidate release consumer route (network-isolated)` (runs `scripts/ci/verify_consumer_checksum_manifest.sh` against `release/` with the same tag identity, after signing and before `Create GitHub Release`).
  - Step: `Check release asset preflight` (fails before publication unless the `release/` directory exactly matches the expected asset contract, every `.sha256` verifies, `checksums.txt` names exactly the published payload assets, and `server.json` points at the generated MCPB checksum).
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
  - Job: `published-release-golden-path` (`Verify the published release journey`; needs `[release-contract, release, publish-crates]`).
    Downloads the public GitHub release assets by tag — not build artifacts from
    the same run — and runs the Linux x86_64 post-publication journey (unchanged)
    plus Windows x86_64 and macOS arm64 published-archive openings. A failure
    turns the release workflow red but cannot unpublish assets.

### Published binary installability

`installer` means `scripts/install.sh` installs the component for that target.
`manual_step` means a release archive exists but the installer does not install
that component. `unsupported` means this release publishes no matching binary;
it is not an installer failure.

<!-- release-installability-matrix:start -->
| Component | Target | Install status | Release asset |
| --- | --- | --- | --- |
| `assay` | `x86_64-unknown-linux-gnu` | `installer` | `assay-v6.8.0-x86_64-unknown-linux-gnu.tar.gz` |
| `assay` | `aarch64-unknown-linux-gnu` | `installer` | `assay-v6.8.0-aarch64-unknown-linux-gnu.tar.gz` |
| `assay` | `x86_64-apple-darwin` | `installer` | `assay-v6.8.0-x86_64-apple-darwin.tar.gz` |
| `assay` | `aarch64-apple-darwin` | `installer` | `assay-v6.8.0-aarch64-apple-darwin.tar.gz` |
| `assay` | `x86_64-pc-windows-msvc` | `installer` | `assay-v6.8.0-x86_64-pc-windows-msvc.zip` |
| `assay-mcp-server` | `x86_64-unknown-linux-gnu` | `manual_step` | `assay-mcp-server-v6.8.0-x86_64-unknown-linux-gnu.tar.gz` |
| `assay-mcp-server` | `aarch64-unknown-linux-gnu` | `manual_step` | `assay-mcp-server-v6.8.0-aarch64-unknown-linux-gnu.tar.gz` |
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
- [ ] **Signed checksum manifest Check**: Confirm the GitHub release includes `checksums.txt` and `checksums.txt.sigstore.json`, and that `checksums.txt` names every other published asset.
- [ ] **Build-provenance bundle Check**: Confirm the GitHub release includes `assay-${VERSION}-build-provenance.sigstore.json` copied from the attest action `bundle-path`.
- [ ] **Release Asset Preflight Check**: Confirm `Check release asset preflight` passed before `Create GitHub Release`; this is the machine-readable asset contract for GitHub release publication.
- [ ] **Published release journey**: Confirm `Verify the published release journey` downloaded the GitHub release assets by tag and ran the Linux x86_64 post-publication journey (unchanged) together with the Windows x86_64 and macOS arm64 published-archive openings (`assay version` against the tag, `assay doctor --format json`, `assay init --preset dev --hello-trace`). This job cannot be satisfied by a same-run build artifact.
- [ ] **Workflow Evidence Check**: Confirm the workflow artifacts include `release-provenance-evidence` with the raw `gh attestation verify --format json` results for each release archive.
- [ ] **Offline Verification Check**: Unpack the proof kit and run `verify-offline.sh --assets-dir /path/to/release-assets` against the downloaded release archives. See [Release Proof Kit](../security/RELEASE-PROOF-KIT.md).
- [ ] **Signed checksum verification**: Run the connected commands in [Signed checksum manifest](#signed-checksum-manifest), or the [network-isolated consumer](#network-isolated-consumer-trustedroot) recipe when air-gap is required. Success prints `Verified OK` from cosign, then `OK` for the selected archive. Failure prints a cosign `Error:`, `checksums.txt does not name`, or `FAILED` from `sha256sum`.

### Signed checksum manifest

Releases cut by this repository's `release.yml` on a tag publish `checksums.txt` (sha256, one name-sorted line per payload asset) and `checksums.txt.sigstore.json` (keyless Sigstore bundle). Verifiers must pin:

- certificate identity: `https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/vX.Y.Z`
- certificate OIDC issuer: `https://token.actions.githubusercontent.com`

`v6.6.1` and earlier do not publish these assets. The first release cut from a tree that contains this signing step is the first one that can be verified this way.

Use cosign v3.1.3 or later (v2.6.5 on the 2.x line) for `verify-blob`. Earlier versions are affected by GHSA-fx35-mq7g-6g98 (verification bypass via public key in a legacy bundle).

The following recipe is a **connected** verify: it downloads with `curl` and lets cosign use its default trust material (which may contact the Sigstore TUF CDN). It is **not** an air-gapped proof. For TrustedRoot under network isolation, use [Network-isolated consumer (TrustedRoot)](#network-isolated-consumer-trustedroot).

```bash
set -euo pipefail
VERSION=vX.Y.Z
ARCHIVE=assay-${VERSION}-x86_64-unknown-linux-gnu.tar.gz
curl -fsSLO "https://github.com/Rul1an/assay/releases/download/${VERSION}/${ARCHIVE}"
curl -fsSLO "https://github.com/Rul1an/assay/releases/download/${VERSION}/checksums.txt"
curl -fsSLO "https://github.com/Rul1an/assay/releases/download/${VERSION}/checksums.txt.sigstore.json"
cosign verify-blob \
  --bundle checksums.txt.sigstore.json \
  --certificate-identity "https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/${VERSION}" \
  --certificate-oidc-issuer https://token.actions.githubusercontent.com \
  checksums.txt
LINE=$(awk -v archive="$ARCHIVE" '$2 == archive { print; found=1 } END { exit !found }' checksums.txt) || {
  echo "checksums.txt does not name ${ARCHIVE}" >&2
  exit 1
}
printf '%s\n' "$LINE" | sha256sum -c -
```

The signed manifest names every published payload. This recipe verifies the selected archive after the signature check; it does not download the rest of the set. A `checksums.txt` that does not name the archive fails with `checksums.txt does not name` and that file.

Success looks like:

```text
Verified OK
assay-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz: OK
```

A bad signature or the wrong identity fails before any per-file hash is trusted:

```text
Error: verifying blob [...]: ...
```

A matching signature over a tampered file fails the hash check:

```text
assay-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz: FAILED
sha256sum: WARNING: 1 computed checksum did NOT match
```

This path talks to Sigstore (Fulcio/Rekor), not to the GitHub attestations API. It does not replace `gh attestation verify` or the [Release Proof Kit](../security/RELEASE-PROOF-KIT.md). The signing step is witnessed by the next real tag-triggered release; a failure there fails the `Create Release` job. `workflow_dispatch` from a branch cannot produce the tag identity and is refused before signing.

CI executes the same tag identity through `scripts/ci/verify_consumer_checksum_manifest.sh`: once against the candidate `release/` directory after signing and before `Create GitHub Release`, and once as a published-assets replay in `published-release-golden-path.yml` (downloads `checksums.txt`, `checksums.txt.sigstore.json`, and the Linux x86_64 archive for `inputs.release_tag`). Both routes read the cosign image from `.github/cosign-image`. Verifying an already-signed tag does not request signing OIDC. Local contract tests stub Docker; they do not measure hosted cryptography. A next-tag candidate log cannot be retrofitted onto `v6.6.2`.

#### Network-isolated consumer (TrustedRoot)

Use this section when the consumer must verify the signed `checksums.txt` without network access during verification. Keep it separate from the connected recipe above and from the proof-kit `verify-offline.sh` path (that wraps `gh attestation verify`).

**Measured route (only).** The v6.6.2 consumer witness ran on **macOS arm64** with Docker pulling the **linux/arm64** cosign image below, then checked the selected archive with host **`shasum -a 256 -c`** (macOS ships `shasum`; this path does not use `sha256sum`). Other hosts and archives are outside that measured route: adapt the archive name and checksum tool yourself if needed; this section does not add a `uname` shell guard and does not claim those adaptations were witnessed.

**Bootstrap (online, once).** Allocate a newly empty parent directory. Set `TUF_ROOT` to a **child** of that parent (for example `$PARENT/tuf-cache`). Do not mount the parent itself as `TUF_ROOT`, do not point at `~/.sigstore`, and do not delete an existing user trust directory for this procedure. Run `cosign initialize` with network allowed until a modern `trusted_root.json` appears under that child. Require that modern TrustedRoot file before continuing; do not treat a legacy fallback root as a pass, and do not treat a separately downloaded root-file hash as a substitute for this bootstrap trust step.

**Verify (network-isolated).** Keep published originals read-only. Run `verify-blob` with `--bundle`, `--trusted-root` (the modern file from bootstrap), and the exact release-workflow identity at the tag plus issuer, under container network isolation. Docker `--network=none` is the isolation shape measured for this documentation path. The deprecated cosign `--offline` flag alone is **not** isolation. Then check the selected archive line from `checksums.txt` with `shasum -a 256 -c`.

Measured cosign image for that witness: tag `ghcr.io/sigstore/cosign/cosign:v3.1.3`; linux/arm64 digest `sha256:153c941dce7e172f66b759a8f5098203902c5b941737c21c7a30cfbe56132f35` (multi-arch index digest `sha256:9e5c2f2edc34351160407ca3416c61855bdf9403c3c5936e0f0be7fc261611b8`). Pin the digest once in `COSIGN_IMAGE` below so both `docker` calls consume the same reference. Those digests name the bytes used in that witness; they are not a claim that every architecture, or a native host cosign on Linux/macOS/Windows, was measured the same way.

```bash
set -euo pipefail
VERSION=vX.Y.Z
ARCHIVE=assay-${VERSION}-aarch64-apple-darwin.tar.gz
# ASSETS holds already-fetched checksums.txt, checksums.txt.sigstore.json, and ARCHIVE (read-only originals).
ASSETS=/path/to/release-assets
PARENT=$(mktemp -d)
mkdir -p "$PARENT/tuf-cache"
# One pin, two consumers (bootstrap + isolated verify).
COSIGN_IMAGE=ghcr.io/sigstore/cosign/cosign@sha256:153c941dce7e172f66b759a8f5098203902c5b941737c21c7a30cfbe56132f35

# 1) Bootstrap (network allowed)
docker run --rm \
  -e TUF_ROOT=/scratch/tuf-cache \
  -v "$PARENT:/scratch" \
  "$COSIGN_IMAGE" \
  initialize
test -f "$PARENT/tuf-cache/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json"

# 2) Verify (isolated)
docker run --rm --network=none \
  -v "$ASSETS:/assets:ro" \
  -v "$PARENT/tuf-cache/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json:/trusted_root.json:ro" \
  "$COSIGN_IMAGE" \
  verify-blob \
    --bundle /assets/checksums.txt.sigstore.json \
    --trusted-root /trusted_root.json \
    --certificate-identity "https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/${VERSION}" \
    --certificate-oidc-issuer https://token.actions.githubusercontent.com \
    /assets/checksums.txt

LINE=$(awk -v archive="$ARCHIVE" '$2 == archive { print; found=1 } END { exit !found }' "$ASSETS/checksums.txt") || {
  echo "checksums.txt does not name ${ARCHIVE}" >&2
  exit 1
}
printf '%s\n' "$LINE" | (cd "$ASSETS" && shasum -a 256 -c -)
```

Success prints `Verified OK`, then one `OK` line for the selected archive. Failure shapes: cosign `Error:`, `checksums.txt does not name`, or `FAILED` from `shasum` (the connected recipe above still documents `sha256sum` failure text for its own path).

A frozen TrustedRoot does not provide ongoing revocation freshness; re-run bootstrap when you need a fresher root. The published release assets and identity pins are the product contract; the container digest and `network=none` shape document one measured consumer path, not broad native-host coverage. Producer CI verifying the signature online is not this consumer observation. This path does not replace `gh attestation verify` or the [Release Proof Kit](../security/RELEASE-PROOF-KIT.md).

- [ ] **Operator Flow Check**: For the compact end-to-end story that connects transcript ingest, shipped `C2` pack evaluation, and proof-kit verification, see [Operator Proof Flow](../guides/operator-proof-flow.md).
- [ ] **Registry Publication Decision**: Treat `release/server.json` as publish-ready input, not proof of an existing live official registry listing.

### 5. Post-publication
- [ ] **Homebrew tap**: Once the release is published, bump `Formula/assay.rb` in
  [`Rul1an/homebrew-tap`](https://github.com/Rul1an/homebrew-tap): the release tag in all four
  `url` lines and the four `sha256` values, copied from the release's
  `assay-vX.Y.Z-<target>.tar.gz.sha256` sidecars for `aarch64-apple-darwin`,
  `x86_64-apple-darwin`, `aarch64-unknown-linux-gnu`, and `x86_64-unknown-linux-gnu`. Land it
  before or alongside the pull request that advances `.github/assay-release-tag`.
  `bash scripts/ci/check-assay-release-pin.sh --published` fails when the formula's version or any
  of its four sha256 values differs from the latest published release, the same way it fails on a
  stale install pin.

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
