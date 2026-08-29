# assay-cli

The `assay` CLI: a policy-as-code gate for MCP agent tool calls, with
verifiable evidence and optional Linux kernel enforcement.

This page is the crate README selected by the `assay-cli` package
manifest. It is not a rendering of the workspace README. crates.io
already selects the crate version, so install commands here stay
unpinned.

Assay ships no single safety score and never claims more than it can prove.

A deny is fail-closed caution, not a verdict on intent; an allow is the decision to forward, never proof the action happened.

## Install

```bash
cargo install assay-cli --locked
```

## Packaged member

The published crate includes
[`evidence_demo_profile.yaml`](evidence_demo_profile.yaml).

## Links

- [crates.io](https://crates.io/crates/assay-cli)
- [docs.rs](https://docs.rs/assay-cli)
- [repository](https://github.com/Rul1an/assay)
