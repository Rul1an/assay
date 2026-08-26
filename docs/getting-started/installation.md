# Installation

The current release is Assay `5.4.0` (`v5.4.0`). Install the CLI from one of the verified channels below.

## CLI

### Unix installer

```bash
curl -fsSL https://getassay.dev/install.sh | sh
```

### Cargo

```bash
cargo install assay-cli --version 5.4.0 --locked
```

The crate is `assay-cli`; the installed binary is `assay`. Releases starting with 3.36.0 declare Rust 1.89 as their MSRV. Repository development currently uses Rust 1.96.

### GitHub release assets

Download the asset for [`v5.4.0`](https://github.com/Rul1an/assay/releases/tag/v5.4.0), verify its published checksum, and place the binary on `PATH`.

Windows x86-64 uses:

```text
assay-v5.4.0-x86_64-pc-windows-msvc.zip
```

Assay does not currently document Homebrew, Scoop, or a public GHCR image as verified release channels.

## Python SDK and pytest plugin

```bash
python -m pip install assay-it
```

CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.

`assay-it` installs the Python SDK and pytest plugin. It does not install the `assay` CLI. The package named `assay` on PyPI is unrelated to this project.

## Verify the CLI

```bash
assay --version
```

Expected output:

```text
assay 5.4.0
```

The generated [agent golden path](../guides/agent-golden-path.md) additionally uses `assay version`, whose release-pinned output is `5.4.0`.

## Development build

Behavior merged after `v5.4.0` is `Unreleased` and is not part of the release claim above.

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
  run: cargo install assay-cli --version 5.4.0 --locked
```

The GitHub Action is available as `Rul1an/assay-action@v2`; follow the [CI integration guide](ci-integration.md) for the repository's current permissions and pinning policy.

## Uninstall

```bash
cargo uninstall assay-cli
python -m pip uninstall assay-it
```

For an installer-script or release-asset installation, remove the installed binary from the location reported by your shell's `command -v assay`.

## Next step

Continue with the [release-pinned agent golden path](../guides/agent-golden-path.md) or the shorter [quick start](quickstart.md).
