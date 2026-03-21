# Wave C2 Review Pack

## Intent

Ship the smallest honest OWASP Agentic subset-pack allowed by `C1`.

## Review Questions

1. Is the shipped rule set exactly `A1-002`, `A3-001`, `A5-001`?
2. Are the open pack and built-in mirror byte-for-byte equivalent?
3. Does the README explicitly state non-goals and avoid overclaim?
4. Are all checks supported by engine `1.0` with zero skip risk?
5. Does the pack stay on control-evidence wording and avoid linkage,
   delegation, temporal, and sandbox claims?

## Validation Commands

- `cargo fmt --check`
- `cargo clippy -q -p assay-evidence --all-targets -- -D warnings`
- `cargo test -q -p assay-evidence --test owasp_agentic_c2_pack`
- `cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping`
- `cargo test -q -p assay-evidence --test pack_engine_manual_test`
- `BASE_REF=origin/main bash scripts/ci/review-wave-c2-owasp-agentic-subset-step1.sh`
- `git diff --check`
