# Contributing to Assay

Assay is a security-critical tool. We maintain high standards for code quality, safety, and performance.

## 1. Development Environment

Assay uses a hybrid build model (Safe Rust + eBPF).

- **Rust**: Latest stable (for host).
- **Nightly Rust**: Specifically for eBPF bytecode generation (managed via Docker).
- **Docker**: Required for eBPF builds on macOS/Windows.

### The Build Toolchain (`xtask`)
We use `cargo xtask` to abstract complex build requirements:

```bash
# 1. Prepare the build environment
cargo xtask build-image

# 2. Build eBPF programs
cargo xtask build-ebpf --docker
```

## 2. Workspace Structure

- `crates/assay-core`: Policy engine and business logic (no-std compatible where possible).
- `crates/assay-ebpf`: Kernel-space programs (LSM, Tracepoints).
- `crates/assay-monitor`: BPF loader and event streamer.
- `crates/assay-cli`: Main entry point.

## 3. Standards

- **Zero Unwraps**: Use `?` or `Result`. Panics are unacceptable in `assay-core`.
- **Clippy**: Must pass `-D warnings`.
- **LSM Verification**: If you touch `assay-ebpf`, you must run `./scripts/verify_lsm_docker.sh`.

## 4. Pull Request Process

1. Feature branch: `feat/description` or `fix/description`.
2. Clean commits using [Conventional Commits](https://www.conventionalcommits.org/).
3. All CI gates (Linux/macOS/Windows) must be green.
# Aya Upgrade Protocol
- Sync  and  versions.
- Run eBPF build successful (docker) for target bpfel-unknown-none.
- Verify with 🚀 Starting Assay Verification...
----------------------------------------------------------------
🛠️  [1/3] Building eBPF bytecode (assay-ebpf)...
----------------------------------------------------------------
eBPF build successful (docker) for target bpfel-unknown-none
----------------------------------------------------------------
🛠️  [2/3] Building assay-cli (userspace)...
----------------------------------------------------------------
🍎 Detected ARM64 (Apple Silicon). Building for target: aarch64-unknown-linux-musl
----------------------------------------------------------------
📝 [3/3] Generating Test Policy (deny.yaml)...
----------------------------------------------------------------
----------------------------------------------------------------
🧪 Starting Runtime Verification...
----------------------------------------------------------------
🍋 Lima VM 'default' detected.
   Running test inside Lima....
