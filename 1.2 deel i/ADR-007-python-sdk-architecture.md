# ADR-007: Python SDK Architecture

## Status

**Accepted** - 2026-01-02

## Context

Assay v1.1.0 is released with Rust CLI and GitHub Action. To reach the 80% of AI/ML developers who work primarily in Python, we need native Python bindings.

Key questions:
1. How to expose Rust code to Python?
2. Which Python versions to support?
3. Sync or async API?
4. How to handle errors across the boundary?

## Decision

### 1. Binding Approach: PyO3 + Maturin

**Choice:** Use PyO3 for Rust → Python bindings, built with Maturin.

**Rationale:**
- Native performance (no subprocess overhead)
- Single codebase (Rust core shared with CLI)
- Type safety preserved across boundary
- Proven by Pydantic v2, Polars, cryptography

**Rejected alternatives:**
- Pure Python + subprocess: Too slow for CI, clunky DX
- gRPC/HTTP wrapper: Extra process, network overhead
- CFFI: More manual work, less ergonomic

### 2. Python Version: 3.9+

**Choice:** Support Python 3.9, 3.10, 3.11, 3.12

**Rationale:**
- Python 3.8 EOL October 2024
- 3.9+ has modern typing features we want
- Covers >95% of active Python users
- Simplifies CI matrix

### 3. API Style: Sync Default

**Choice:** Synchronous API only in v1.2. Async deferred to v1.3.

**Rationale:**
- 90% use case is CI/CD scripts (sync)
- pytest default runner is sync
- Adding async later is backward compatible
- Ship fast > feature complete

**v1.3 plan:** Add `assay.aio` module with async variants.

### 4. Error Handling: Python Exceptions

**Choice:** Wrap all Rust errors in Python exceptions.

```python
# Rust panic → RuntimeError
# Invalid input → ValueError  
# File not found → FileNotFoundError
# Policy parse error → ValueError with message
```

**Rationale:**
- Pythonic error handling (try/except)
- Clear error messages with context
- No Rust internals leak to users

### 5. Package Structure

```
assay/
├── __init__.py          # Public API exports
├── _native.cpython-*.so # Compiled Rust module
├── pytest.py            # pytest plugin
└── py.typed             # PEP 561 type marker
```

**Rationale:**
- `_native` prefix indicates internal module
- Public API in pure Python for documentation
- Type hints via py.typed + stubs

### 6. Distribution: Wheels + Source

**Choice:** Publish pre-built wheels for common platforms, with source fallback.

Wheel matrix:
- Linux x86_64 (manylinux2014)
- Linux aarch64 (manylinux2014)
- macOS x86_64
- macOS arm64
- Windows x86_64

**Rationale:**
- Most users get fast install (no Rust toolchain)
- Source dist for exotic platforms
- Maturin handles complexity

## Consequences

### Positive

- Native performance in Python
- Single source of truth (Rust core)
- Type safety via dataclasses + type hints
- pytest integration out of box
- Familiar Pythonic API

### Negative

- Complex build pipeline (CI matrix for wheels)
- manylinux compatibility requires attention
- No async support initially
- Users on exotic platforms need Rust toolchain

### Risks

| Risk | Mitigation |
|------|------------|
| manylinux build fails | Test early Week 1, fallback to source |
| Memory issues at boundary | Extensive testing, Evals Lead validation |
| Windows wheel issues | Dedicated debug time Week 3 |

## Implementation

See: `python-sdk/` directory with:
- `Cargo.toml` - Rust dependencies
- `src/lib.rs` - PyO3 bindings
- `assay/__init__.py` - Python wrapper
- `pyproject.toml` - Package config

## References

- [PyO3 User Guide](https://pyo3.rs/)
- [Maturin Documentation](https://www.maturin.rs/)
- [PEP 561 - Type Stubs](https://peps.python.org/pep-0561/)
- ADR-006: DSL v1.1 Operators
