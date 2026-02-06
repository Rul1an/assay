# Installation

Install Assay on your system.

---

## Quick Install

=== "pip (Python)"

    ```bash
    pip install assay-it
    ```

    Requires Python 3.9+. Installs the `assay` CLI globally.

=== "cargo (Rust)"

    ```bash
    cargo install assay-cli --locked
    ```

    **Note:** The crate is named `assay-cli`, but the binary is `assay`.
    Requires Rust 1.70+. Builds from source (~2 minutes).

=== "Homebrew (macOS)"

    ```bash
    brew install rul1an/tap/assay
    ```

    Installs pre-built binary.

=== "Binary (Linux/macOS)"

    ```bash
    curl -sSL https://assay.dev/install.sh | sh
    ```

    Installs `assay` to `~/.assay/bin` and updates your PATH.

---

## Verify Installation

```bash
assay --version
```

Expected output:
```
assay 0.9.0
```

---

## Platform-Specific Notes

### macOS

If you see a security warning:

```bash
# Allow the binary
xattr -d com.apple.quarantine /usr/local/bin/assay
```

### Windows

=== "Cargo"

    ```powershell
    cargo install assay-cli --locked
    ```

=== "Scoop"

    ```powershell
    scoop bucket add assay https://github.com/Rul1an/scoop-assay
    scoop install assay
    ```

=== "Binary"

    Download `assay-windows-x86_64.zip` from [GitHub Releases](https://github.com/Rul1an/assay/releases) and add to PATH.

### Docker

```bash
docker pull ghcr.io/rul1an/assay:latest

# Run with volume mount
docker run -v $(pwd):/workspace ghcr.io/rul1an/assay:latest \
    run --config /workspace/eval.yaml
```

---

## Development Installation

For contributors or those who want the latest features:

```bash
# Clone the repo
git clone https://github.com/Rul1an/assay.git
cd assay

# Build in release mode
cargo build --release

# Run from target directory
./target/release/assay --version
```

---

## CI Installation

### GitHub Actions

```yaml
- name: Install Assay
  run: cargo install assay-cli --locked

# Or use our action (includes caching)
- uses: assay-dev/assay-action@v1
```

### GitLab CI

```yaml
before_script:
  - cargo install assay-cli --locked
```

### Azure Pipelines

```yaml
- script: cargo install assay-cli --locked
  displayName: 'Install Assay'
```

---

## Uninstall

=== "pip"

    ```bash
    pip uninstall assay-it
    ```

=== "cargo"

    ```bash
    cargo uninstall assay-cli
    ```

=== "Homebrew"

    ```bash
    brew uninstall assay
    ```

---

## Troubleshooting

### `cargo install` fails with SSL errors

```bash
# Update certificates
sudo apt-get update && sudo apt-get install -y ca-certificates
```

### `pip install` fails with permission errors

```bash
# Use --user flag
pip install --user assay

# Or use pipx for isolated installation
pipx install assay
```

### Binary not found after installation

Ensure your PATH includes:

- **Cargo:** `~/.cargo/bin`
- **pip:** `~/.local/bin`
- **Homebrew:** `/opt/homebrew/bin` (Apple Silicon) or `/usr/local/bin` (Intel)

---

## Next Steps

[:octicons-arrow-right-24: Quick Start — Run your first test](quickstart.md)
