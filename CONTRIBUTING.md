# Contributing to Assay

Security-critical tool. High standards for code quality, safety, and performance.

## Development Setup

```bash
# Clone
git clone https://github.com/Rul1an/assay.git
cd assay

# Build
cargo build --workspace

# Test
cargo test --workspace

# Lint
cargo clippy --workspace -- -D warnings
```

### eBPF (Linux only)

```bash
cargo xtask build-image
cargo xtask build-ebpf --docker
./scripts/verify_lsm_docker.sh
```

### Python SDK

```bash
cd assay-python-sdk
pip install maturin
maturin develop
pytest python/tests/
```

## Workspace Structure

| Crate | Purpose |
|-------|---------|
| `assay-cli` | CLI binary, commands, reporting |
| `assay-core` | Policy engine, trace replay, storage |
| `assay-evidence` | Evidence bundles, verification, lint |
| `assay-metrics` | Evaluation metrics (MustContain, ArgsValid, etc.) |
| `assay-mcp-server` | MCP proxy with policy enforcement |
| `assay-monitor` | eBPF loader, event streaming |
| `assay-policy` | Policy compilation (Tier 1/2) |
| `assay-ebpf` | Kernel-space LSM programs |
| `assay-common` | Shared types (no_std compatible) |
| `assay-sim` | Attack simulation |
| `assay-python-sdk` | Python bindings + pytest plugin |

## Code Standards

- **No panics**: Use `?` and `Result`. No `.unwrap()` in `assay-core`.
- **Clippy clean**: `cargo clippy -- -D warnings`
- **Formatted**: `cargo fmt`
- **Tested**: New features require tests

## Testing

```bash
# All tests
cargo test --workspace

# Specific crate
cargo test -p assay-core
cargo test -p assay-evidence

# E2E tests
cargo test -p assay-cli --test evidence_test

# Python
cd assay-python-sdk && pytest
```

## Pull Requests

1. Branch: `feat/description` or `fix/description`
2. Commits: [Conventional Commits](https://www.conventionalcommits.org/)
3. CI must pass (Linux/macOS/Windows)
4. Update docs if behavior changes

## Aya/eBPF Upgrades

When upgrading `aya` or `aya-ebpf`:

1. Sync versions across all `Cargo.toml`
2. Build: `cargo xtask build-ebpf --docker`
3. Verify: `./scripts/verify_lsm_docker.sh`

## Pre-commit

```bash
pip install pre-commit
pre-commit install
```

Runs `cargo fmt`, `cargo clippy`, and `typos` on commit.
