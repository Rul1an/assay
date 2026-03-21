# Wave C1 Review Pack

Review intent:
- prove the strongest honest assurance level the current engine can support for
  `ASI01`, `ASI03`, and `ASI05`
- block any later `C2` pack from overstating skipped, unsupported, or missing-signal logic

Key review questions:
- Does the mapping doc distinguish `yaml-only`, `engine gap`, and `signal gap`?
- Is each candidate rule assigned exactly one `Max Provable Level`?
- Does the test suite prove that unsupported `security`-pack logic skips?
- Are claimed signal gaps backed by bundle fixtures or evidence-flow probes?
- Does anything in this diff widen built-in packs or pack-engine code?

Validation commands:
- `cargo fmt --check`
- `cargo clippy -q -p assay-evidence --all-targets -- -D warnings`
- `cargo test -q -p assay-evidence --test owasp_agentic_c1_mapping`
- `cargo test -q -p assay-evidence --test pack_engine_manual_test`
- `BASE_REF=origin/main bash scripts/ci/review-wave-c1-owasp-agentic-mapping-step1.sh`
- `git diff --check`
