# Verdict

**Verdict** is a CI-first **PR regression gate** for **RAG pipelines**. It outputs **JUnit XML** + **SARIF** so failures show up natively in CI/PRs.

MVP focus (v0.1–v0.3):
- **VCR cache** (record/replay LLM calls) for fast PR loops
- **rerun-on-failure** + **flake classification** + **quarantine lane**
- CI-native reports: **JUnit** + **SARIF**

Not MVP (v0.5+):
- multi-agent / tool-calling / trajectory evaluation
- incremental fingerprints
- baseline/compare gating

## Install (dev)
```bash
cargo install --path crates/verdict-cli
```

## Quickstart
```bash
verdict init
verdict ci --rerun-failures 2
```
