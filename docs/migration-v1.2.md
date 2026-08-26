# Migration Guide: v1.1.0 to v1.2.0

Version 1.2.0 introduces the Native Python SDK and Baseline Management system.

## Breaking Changes

### 1. Renamed `threshold` to `min_coverage`
If you used the `--threshold` flag in the CLI or `threshold` in config for checking coverage, it has been renamed to clearer `min_coverage`.

**Old (v1.1):**
```bash
assay coverage --threshold 80.0
```

**New (v1.2):**
```bash
assay coverage --min-coverage 80.0
```

### 2. Experimental Flags Removed
The `--experimental` flag is no longer required for the `explain` command, as it is now stable.

**Old (v1.1):**
```bash
assay explain --experimental
```

**New (v1.2):**
```bash
assay explain
```

## New Features

### Python SDK

> Correction (2026-08-13): the shipped Python distribution is `assay-it`; the PyPI package named `assay` is unrelated to this project.

Install the SDK and pytest plugin:
```bash
pip install assay-it
```

CPython 3.12 on macOS x86_64/arm64 and Linux x86_64; other interpreters and platforms are not claimed.

See [Python Quickstart](getting-started/python-quickstart.md) for details.

### Baseline Management
New commands for regression testing:
- `assay baseline record`
- `assay baseline check`

See [Baseline Guide](guides/baseline-guide.md).
