# Fuzz Targets

This directory contains `cargo-fuzz` harnesses for parser and bundle-reader surfaces
that are easy to regress silently:

- `policy_yaml`: fuzzes YAML policy parsing for both eval config and MCP policy shapes
- `bundle_reader`: fuzzes evidence-chain verification against arbitrary tar.gz bytes under small,
  explicit resource ceilings; deterministic fail-closed classifications live in
  `crates/assay-evidence/tests/verifier_fail_closed_properties.rs`

Examples:

```bash
cd fuzz
cargo fuzz run policy_yaml
cargo fuzz run bundle_reader
```
