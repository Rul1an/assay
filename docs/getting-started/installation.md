# Installation

The current release is Assay `6.6.1` (`v6.6.1`). Install the CLI from one of the verified channels below.

## CLI

### Unix installer

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

### Cargo

```bash
cargo install assay-cli --version 6.6.1 --locked
```

The crate is `assay-cli`; the installed binary is `assay`. Releases starting with 3.36.0 declare Rust 1.89 as their MSRV. Repository development currently uses Rust 1.96.

### GitHub release assets

Download the asset for [`v6.6.1`](https://github.com/Rul1an/assay/releases/tag/v6.6.1), verify its published checksum, and place the binary on `PATH`.

Windows x86-64 uses:

```text
assay-v6.6.1-x86_64-pc-windows-msvc.zip
```

Assay documents the container image below as a verified release channel. Homebrew and Scoop remain unsupported.

## Container image (assay-mcp-server)

The `v6.6.1` image index is `ghcr.io/rul1an/assay-mcp-server@sha256:65713e916d2004aeb54b7836a2f0bbccf0343dbf5590cf1a92ab16e080fb4a37` (tags such as `v6.6.1`, `6.6`, and `latest` are convenience aliases; the digest is the pinned reference).

Pull and run the multi-arch `assay-mcp-server` image by index digest:

```bash
docker run --rm ghcr.io/rul1an/assay-mcp-server@sha256:65713e916d2004aeb54b7836a2f0bbccf0343dbf5590cf1a92ab16e080fb4a37 --version
```

The image runs as uid:gid 65532:65532 (non-root) on a minimal base, and the index contains both linux/amd64 and linux/arm64 images.

Verify SLSA provenance and CycloneDX SBOM attestations:

```bash
gh attestation verify oci://ghcr.io/rul1an/assay-mcp-server@sha256:65713e916d2004aeb54b7836a2f0bbccf0343dbf5590cf1a92ab16e080fb4a37 -R Rul1an/assay --predicate-type https://slsa.dev/provenance/v1
gh attestation verify oci://ghcr.io/rul1an/assay-mcp-server@sha256:65713e916d2004aeb54b7836a2f0bbccf0343dbf5590cf1a92ab16e080fb4a37 -R Rul1an/assay --predicate-type https://cyclonedx.org/bom
```

Verified status means [release run 35372454372](https://github.com/Rul1an/assay/actions/runs/35372454372) pulled the image by digest, verified attestations, and executed `--version` on both architectures.

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
