# Changelog

All notable changes to Assay will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.2.0] - 2026-01-XX

### Added

#### Python SDK

Native Python bindings via PyO3. Install with pip:

```bash
pip install assay
```

Usage:

```python
from assay import Policy, CoverageAnalyzer

policy = Policy.from_file("policy.yaml")
analyzer = CoverageAnalyzer(policy)

report = analyzer.analyze(
    traces=[{"tools": ["Search", "Create"]}],
    threshold=80.0
)

assert report.passed, f"Coverage {report.coverage}% below threshold"
```

Features:
- `Policy` - Load and validate policies
- `CoverageAnalyzer` - Analyze trace coverage
- `TraceExplainer` - Step-by-step trace debugging
- `check_coverage()` - Convenience function
- pytest integration via `assay.pytest`

Supported platforms:
- Linux x86_64 and aarch64
- macOS x86_64 and arm64 (Apple Silicon)
- Windows x86_64
- Python 3.9, 3.10, 3.11, 3.12

#### Baseline Management

Track coverage over time and detect regressions:

```bash
# Save baseline (on main branch)
assay baseline save --policy policy.yaml --traces traces/

# Check for regressions (on PR)
assay baseline diff --policy policy.yaml --traces traces/
```

Features:
- Automatic git commit/branch detection
- Coverage delta calculation
- Tool regression detection
- Rule regression detection
- GitHub Actions annotation format
- Configurable regression threshold

#### Trace Explanation (Promoted to Stable)

The `assay explain` command is now stable (previously experimental):

```bash
# No more --experimental flag needed
assay explain --policy policy.yaml --trace trace.json
```

Changes:
- Removed `--experimental` flag requirement
- Moved from `assay_core::experimental::explain` to `assay_core::explain`
- Full documentation added

### Changed

- **Python 3.9+ required** for Python SDK (3.8 is EOL)
- Improved error messages with actionable hints
- Better performance for large traces

### Migration from v1.1

#### CLI Changes

```bash
# v1.1 (old)
assay explain --experimental --policy p.yaml --trace t.json

# v1.2 (new)
assay explain --policy p.yaml --trace t.json
```

The `--experimental` flag is silently ignored in v1.2 for backward compatibility.

#### Rust API Changes

```rust
// v1.1
#[cfg(feature = "experimental")]
use assay_core::experimental::explain::TraceExplainer;

// v1.2
use assay_core::explain::TraceExplainer;
```

### Deprecated

- The `experimental` feature flag is deprecated and will be removed in v1.3

### Fixed

- Windows binary extension detection in tests
- Clippy warnings for `wrong_self_convention`
- Large enum variant memory optimization

---

## [1.1.0] - 2026-01-02

### Added

#### Policy DSL v2 - Temporal Constraints

New sequence operators for complex agent workflow validation:

- **`max_calls`** - Rate limiting per tool
- **`after`** - Post-condition enforcement
- **`never_after`** - Forbidden sequences
- **`sequence`** - Exact ordering with strict mode

#### Aliases

Define tool groups for cleaner policies.

#### Coverage Metrics

New `assay coverage` command for CI/CD integration.

#### GitHub Action

```yaml
- uses: Rul1an/assay/assay-action@v1
  with:
    policy: policies/agent.yaml
    traces: traces/
    min-coverage: 80
```

#### One-liner Installation

```bash
curl -sSL https://assay.dev/install.sh | sh
```

### Changed

- Policy version bumped to `1.1`

---

## [1.0.0] - 2025-12-30

### Added

- Initial stable release
- Policy DSL v1.0 with allow/deny lists
- Sequence rules: `require`, `eventually`, `before`, `blocklist`
- MCP server integration
- CLI: `assay check` command

---

## [0.9.0] - 2025-12-28

### Added

- Pre-release candidate
- Rebrand from "Verdict" to "Assay"

---

[1.2.0]: https://github.com/Rul1an/assay/compare/v1.1.0...v1.2.0
[1.1.0]: https://github.com/Rul1an/assay/compare/v1.0.0...v1.1.0
[1.0.0]: https://github.com/Rul1an/assay/compare/v0.9.0...v1.0.0
[0.9.0]: https://github.com/Rul1an/assay/releases/tag/v0.9.0
