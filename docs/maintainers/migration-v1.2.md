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
You can now install Assay as a Python library:
```bash
pip install assay
```

See [Python Quickstart](python-quickstart.md) for details.

### Baseline Management
New commands for regression testing:
- `assay baseline record`
- `assay baseline check`

See [Baseline Guide](baseline-guide.md).
