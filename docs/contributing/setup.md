# Developer Setup: The First 5 Minutes

## 1. Prerequisites
- **Rust**: `curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh`
- **Docker**: (Required for non-Linux hosts)
- **Repo**: `git clone git@github.com:Rul1an/assay.git && cd assay`

## 2. Prepare the Toolchain
Assay uses a custom builder image to ensure consistent eBPF bytecode generation.

```bash
cargo xtask build-image
```

## 3. Build & Verify
Build the host binary and the eBPF kernel module, then run the verification suite.

```bash
# Build eBPF (using Docker)
cargo xtask build-ebpf --docker

# Build Host
cargo build --workspace

# Run E2E Verification (requires sudo/root/lima)
./scripts/verify_lsm_docker.sh
```

## 4. Quality Gates
Before pushing a PR, ensure these pass:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```
