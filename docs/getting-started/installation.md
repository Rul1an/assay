# Installation

The current release is Assay `6.6.1` (`v6.6.1`). Install the CLI from one of the verified channels below.

## CLI

### Unix installer

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

### Homebrew

On macOS (arm64, x86_64) and Linux (arm64, x86_64):

```bash
brew install Rul1an/tap/assay
assay --version
```

The formula in [`Rul1an/homebrew-tap`](https://github.com/Rul1an/homebrew-tap) installs the prebuilt release archive, pinned by the release's published sha256. `brew upgrade assay` picks up later releases.

### Cargo

```bash
cargo install assay-cli --version 6.6.1 --locked
```

The crate is `assay-cli`; the installed binary is `assay`. Releases starting with 3.36.0 declare Rust 1.89 as their MSRV. Repository development currently uses Rust 1.96.

### GitHub release assets

Download the asset for [`v6.6.1`](https://github.com/Rul1an/assay/releases/tag/v6.6.1), verify its published checksum, and place the binary on `PATH`.

Releases cut after this page's `v6.6.1` pin also publish a signed `checksums.txt`. When `cosign` is on `PATH` and reports v3.1.3 or later (v2.6.5 on the 2.x line), `scripts/install.sh` verifies that manifest against the release workflow identity at the tag before it trusts any per-file hash. When `cosign` is present but older or unparsable, the installer refuses that signature check and stops (GHSA-fx35-mq7g-6g98). When `cosign` is absent, the installer prints `verification=signed_manifest_skipped reason=cosign_not_installed` and continues with the per-file `.sha256` sidecar. It never skips that check silently.

To verify a published archive yourself (replace `vX.Y.Z` with the tag you downloaded):

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

Use cosign v3.1.3 or later (v2.6.5 on the 2.x line). Earlier versions are affected by GHSA-fx35-mq7g-6g98 (verification bypass via public key in a legacy bundle).

The signed manifest names every published payload. This recipe verifies the selected archive after the signature check; it does not download the rest of the set. Success prints `Verified OK`, then one `OK` line for that archive. A bad signature prints a cosign `Error:` and stops. A `checksums.txt` that does not name the archive fails with `checksums.txt does not name` and that file. A missing or tampered archive prints `FAILED` from `sha256sum`. Pin exactly that certificate identity and issuer; do not accept a signature bound to a branch ref.

`v6.6.1` and earlier have per-file `.sha256` sidecars only. The same installer then reports `verification=signed_manifest_unavailable reason=checksums.txt_not_published` when `cosign` is present, and still verifies the sidecar.

See [release.md](../reference/release.md#signed-checksum-manifest) for the operator checklist.

Windows x86-64 uses:

```text
assay-v6.6.1-x86_64-pc-windows-msvc.zip
```

Assay documents the container image below as a verified release channel. Scoop remains unsupported.

## Container image (assay-mcp-server)

The `v6.6.1` image index is `ghcr.io/rul1an/assay-mcp-server@sha256:8143b45ea06783d3c371919a5ce86b343ab7ad8565e15b07889b8fac554f58a2` (tags such as `v6.6.1`, `6.6`, and `latest` are convenience aliases; the digest is the pinned reference).

Pull and run the multi-arch `assay-mcp-server` image by index digest:

```bash
docker run --rm ghcr.io/rul1an/assay-mcp-server@sha256:8143b45ea06783d3c371919a5ce86b343ab7ad8565e15b07889b8fac554f58a2 --version
```

The image runs as uid:gid 65532:65532 (non-root) on a minimal base, and the index contains both linux/amd64 and linux/arm64 images.

Verify SLSA provenance and CycloneDX SBOM attestations:

```bash
gh attestation verify oci://ghcr.io/rul1an/assay-mcp-server@sha256:8143b45ea06783d3c371919a5ce86b343ab7ad8565e15b07889b8fac554f58a2 -R Rul1an/assay --predicate-type https://slsa.dev/provenance/v1
gh attestation verify oci://ghcr.io/rul1an/assay-mcp-server@sha256:8143b45ea06783d3c371919a5ce86b343ab7ad8565e15b07889b8fac554f58a2 -R Rul1an/assay --predicate-type https://cyclonedx.org/bom
```

Verified status means [release run 35395488916](https://github.com/Rul1an/assay/actions/runs/35395488916) pulled the image by digest, verified attestations, and executed `--version` on both architectures.

## Python SDK and pytest plugin

```bash
python -m pip install assay-it
```

CPython 3.12, 3.13, and 3.14 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.

`assay-it` installs the Python SDK and pytest plugin. It does not install the `assay` CLI. The package named `assay` on PyPI is unrelated to this project.

## Verify the CLI

```bash
assay --version
```

Expected output:

```text
assay 6.6.1
```

The generated [agent golden path](../guides/agent-golden-path.md) additionally uses `assay version`, whose release-pinned output is `6.6.1`.

### Verify an evidence bundle offline

To verify an evidence bundle offline without network access:

```bash
assay evidence verify-privileged-mcp-action <bundle> --format json
```

Both outcomes emit a JSON document adhering to the [`assay.privileged_mcp_action.verify.report.v0`](../profiles/privileged-mcp-action/v0.md) report schema:

- **Valid** (exit code `0`): `bundle_integrity: pass` and `verdict: valid`.
- **Integrity failure or invalid verdict** (exit code `2`): on integrity failure, `bundle_integrity: fail`, `reason_code: E_EVIDENCE_INTEGRITY`, and the `verdict` key is omitted; on an invalid verdict with integrity pass, `bundle_integrity: pass` and `verdict: invalid`.

The report is experimental v0; verification recomputes the carried bytes only.

## Development build

Behavior merged after `v6.6.1` is `Unreleased` and is not part of the release claim above.

```bash
git clone https://github.com/Rul1an/assay.git
cd assay
cargo build --release
./target/release/assay --version
```

## CI

For source installation in CI:

```yaml
- name: Install Assay
  run: cargo install assay-cli --version 6.6.1 --locked
```

The GitHub Action is available as `Rul1an/assay-action@v3`; follow the [CI integration guide](ci-integration.md) for the repository's current permissions and pinning policy.

## Uninstall

```bash
cargo uninstall assay-cli
python -m pip uninstall assay-it
```

For an installer-script or release-asset installation, remove the installed binary from the location reported by your shell's `command -v assay`.

## Next step

Continue with the [release-pinned agent golden path](../guides/agent-golden-path.md) or the shorter [quick start](quickstart.md).
