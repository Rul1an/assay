# Contributing

We appreciate your interest in contributing to Assay! This document outlines our conventions and standards. As a security-critical tool for AI agents, we enforce strict quality gates.

## Development Workflow

### 1. Prerequisites
*   **Rust**: Latest stable.
*   **Python**: v3.10+ (for SDK bindings).
*   **Tools**: `cargo-deny`, `cargo-nextest` (optional but recommended).

### 2. Workspace Structure
Assay is a Cargo workspace divided into:
*   **`crates/assay-core`**: Pure Rust business logic. Zero IO where possible.
*   **`crates/assay-cli`**: The CLI binary. Handles IO, config parsing, and user interaction.
*   **`crates/assay-mcp-server`**: The MCP protocol implementation.
*   **`assay-python-sdk`**: PyO3 bindings for Python integration.

### 3. Guidelines

#### Code Quality
*   **Strict Clippy**: We run clippy with `-D warnings`.
    ```bash
    cargo clippy --workspace --all-targets -- -D warnings
    ```
*   **Formatting**: `cargo fmt` is enforced in CI.
*   **No Unwraps**: Avoid `.unwrap()` in `assay-core`. Use `?` propagation or `.expect("invariant: explanation")` only if mathematically impossible to fail.

#### Testing
*   **Unit Tests**: Co-located in `src/`.
*   **Integration Tests**: Located in `tests/`. We use `try-cmd` or `assert_cmd` for CLI snapshot testing.
*   **Determinism**: Policy decisions MUST be deterministic.

#### Commits
We use [Conventional Commits](https://www.conventionalcommits.org/):
*   `feat(core): add new validator`
*   `fix(cli): resolve path expansion bug`
*   `docs: update README`
*   `chore: bump dependencies`

## Pull Request Process

1.  **Fork & Branch**: Create a feature branch (`feat/my-feature`).
2.  **Test Locally**: Ensure `cargo test` passes.
3.  **Submit PR**: Open a PR against `main`.
4.  **CI Gates**:
    *   `fmt`: formatting check.
    *   `clippy`: lint check.
    *   `test`: unit/integration tests (`linux`, `macos`, `windows`).
    *   `audit`: `cargo deny` check (licenses/advisories).

## Python SDK

If modifying `assay-python-sdk`:

1.  Setup typical venv: `python -m venv .venv && source .venv/bin/activate`
2.  Install `maturin`: `pip install maturin`
3.  Build & Install dev version:
    ```bash
    maturin develop --manifest-path assay-python-sdk/Cargo.toml
    ```
4.  Run tests: `pytest`

## Release Process

Releases are automated via GitHub Actions on `v*` tags.
1.  Update `CHANGELOG.md`.
2.  Bump versions in `Cargo.toml`.
3.  Tag (e.g., `git tag v1.3.1`).
4.  Push tag.

CI will automatically:
*   Build binaries (Linux/macOS/Windows).
*   Publish to Crates.io (Trusted Publishing).
*   Publish to PyPI (`assay-it`).
