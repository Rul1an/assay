# Wave C1 Inventory

Step: `wave-c1-owasp-agentic-mapping-step1`

Base:
- `origin/main`
- starting HEAD: `a63b78c7fd8af9a03db28e6a1bb3bf6a2370a997`

Scope freeze:
- docs-only feasibility map in `docs/security/`
- test-only probe packs under `crates/assay-evidence/tests/fixtures/packs/`
- one targeted feasibility test file
- wave artifacts and reviewer gate

Explicitly out of scope:
- `packs/open/*`
- `crates/assay-evidence/packs/*`
- built-in registration in `crates/assay-evidence/src/lint/packs/mod.rs`
- engine changes in `checks.rs`, `schema.rs`, `executor.rs`, or `engine.rs`

Touched file allowlist:
- `docs/security/OWASP-AGENTIC-A1-A3-A5-C1-MAPPING.md`
- `crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a1-probe.yaml`
- `crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a3-probe.yaml`
- `crates/assay-evidence/tests/fixtures/packs/owasp-agentic-a5-probe.yaml`
- `crates/assay-evidence/tests/owasp_agentic_c1_mapping.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-c1-owasp-agentic-mapping-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-c1-owasp-agentic-mapping-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-c1-owasp-agentic-mapping-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c1-owasp-agentic-mapping-step1.md`
- `scripts/ci/review-wave-c1-owasp-agentic-mapping-step1.sh`
