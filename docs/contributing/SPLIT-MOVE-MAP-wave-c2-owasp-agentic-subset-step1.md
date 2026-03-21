# Wave C2 Move Map

This wave does not move runtime logic. It introduces a shipped subset-pack and
its mirror/registration path.

## Mapping

- Open pack source of truth:
  - `packs/open/owasp-agentic-control-evidence-baseline/pack.yaml`
- Mirrored built-in YAML:
  - `crates/assay-evidence/packs/owasp-agentic-control-evidence-baseline.yaml`
- Built-in registration:
  - `crates/assay-evidence/src/lint/packs/mod.rs`
- Contract tests:
  - `crates/assay-evidence/tests/owasp_agentic_c2_pack.rs`
- Review gate:
  - `scripts/ci/review-wave-c2-owasp-agentic-subset-step1.sh`
