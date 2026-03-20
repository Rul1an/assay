# Fuzz Targets

This directory contains `cargo-fuzz` harnesses for parser and bundle-reader surfaces
that are easy to regress silently:

- `policy_yaml`: fuzzes YAML policy parsing for both eval config and MCP policy shapes
- `bundle_reader`: fuzzes replay bundle verification against arbitrary tar.gz bytes

Examples:

```bash
cd fuzz
cargo fuzz run policy_yaml
cargo fuzz run bundle_reader
```
