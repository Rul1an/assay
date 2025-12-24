# Contributing to Assay

We enforce strict code quality standards to ensure reliability in CI environments.

## Development

1.  **Rust Toolchain**: Use the latest stable release.
2.  **Formatting**: `cargo fmt` must pass.
3.  **Linting**: Zero tolerance for warnings.
    ```bash
    cargo clippy --workspace --all-targets -- -D warnings
    ```
4.  **Testing**: All tests must pass, including integration tests.
    ```bash
    cargo test --workspace
    ```

## Pull Requests

*   **Atomic Commits**: Keep changes focused.
*   **Conventional Commits**: Use conventional commit messages (e.g., `feat:`, `fix:`, `chore:`).
*   **Regression Check**: Run the CI gate locally before pushing.
    ```bash
    # Example
    cargo run --release -- ci --config examples/ci-regression-gate/eval.yaml ...
    ```

## Architecture

*   **Core**: Business logic goes in `crates/assay-core`.
*   **CLI**: Interface logic goes in `crates/assay-cli`.
*   **No Flakiness**: Any test that relies on network or external state must utilize the `replay` mechanism or be mocked.
