# Wave C2 Inventory

## Scope

Ship a narrow OWASP Agentic control-evidence subset pack derived from `C1`.

## Included Files

- `packs/open/owasp-agentic-control-evidence-baseline/pack.yaml`
- `packs/open/owasp-agentic-control-evidence-baseline/README.md`
- `packs/open/owasp-agentic-control-evidence-baseline/LICENSE`
- `crates/assay-evidence/packs/owasp-agentic-control-evidence-baseline.yaml`
- `crates/assay-evidence/src/lint/packs/mod.rs`
- `crates/assay-evidence/tests/owasp_agentic_c2_pack.rs`
- `docs/contributing/SPLIT-INVENTORY-wave-c2-owasp-agentic-subset-step1.md`
- `docs/contributing/SPLIT-CHECKLIST-wave-c2-owasp-agentic-subset-step1.md`
- `docs/contributing/SPLIT-MOVE-MAP-wave-c2-owasp-agentic-subset-step1.md`
- `docs/contributing/SPLIT-REVIEW-PACK-wave-c2-owasp-agentic-subset-step1.md`
- `scripts/ci/review-wave-c2-owasp-agentic-subset-step1.sh`

## Rule Freeze

- `A1-002`
- `A3-001`
- `A5-001`

## Explicitly Out Of Scope

- `A1-001`
- `A3-002`
- `A3-003`
- `A5-002`
- any engine changes
- any new emitters
- any runtime or CLI changes
