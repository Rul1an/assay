# Registry Client Step1 Checklist (Freeze)

Scope lock:
- docs + reviewer gate script only
- no `crates/assay-registry/tests/registry_client.rs` edits
- no workflow edits

## Required Step1 outputs

- [ ] `docs/contributing/SPLIT-PLAN-registry-client-wave11.md`
- [ ] `docs/contributing/SPLIT-CHECKLIST-registry-client-step1.md`
- [ ] `docs/contributing/SPLIT-REVIEW-PACK-registry-client-step1.md`
- [ ] `scripts/ci/review-registry-client-step1.sh`

## Inventory completeness

- [ ] scenario inventory for all current tests captured
- [ ] shared setup/helpers inventory captured
- [ ] external dependency/flakiness surface documented
- [ ] target Step2 mechanical split map documented
- [ ] behavior invariants explicitly frozen

## Gate requirements

- [ ] diff allowlist only (Step1 files above)
- [ ] workflow-ban (`.github/workflows/*` forbidden)
- [ ] `cargo fmt --check`
- [ ] `cargo test -p assay-registry --tests`

## Definition of done

- [ ] reviewer script passes with `BASE_REF=origin/main`
- [ ] no non-allowlisted file changes
- [ ] Step2 boundaries are explicit and reviewable
