# Developer Setup: The First 5 Minutes

## 1. Prerequisites
- **Rust**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Native eBPF toolchain**: LLVM/Clang, nightly Rust, `rust-src`, and `bpf-linker`
- **Docker**: Optional fallback for non-Linux hosts
- **Repo**: `git clone git@github.com:Rul1an/assay.git && cd assay`

## 2. Prepare the Toolchain
Assay's delegated runner path uses native `bpf-linker` builds so cold Docker image builds stay out of the proof hot path.

```bash
rustup toolchain install nightly-2026-01-01 --profile minimal
rustup component add rust-src --toolchain nightly-2026-01-01
rustup run nightly-2026-01-01 cargo install bpf-linker --version 0.10.3 --locked
```

## 3. Build & Verify
Build the host binary and the eBPF kernel module, then run the verification suite.

```bash
# Build eBPF (native bpf-linker path)
cargo xtask build-ebpf --release --no-docker

# Build Host
cargo build --workspace

# Run E2E Verification (requires sudo/root/lima)
./scripts/verify_lsm_docker.sh
```

If the native toolchain is not available on a local machine, the Docker fallback remains: `cargo xtask build-image && cargo xtask build-ebpf --docker`.

## 4. Quality Gates
Before pushing a PR, ensure these pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
