# Coverage Command Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-coverage-command-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-coverage-command-step3.md`
- `scripts/ci/review-coverage-command-step3.sh`
- no code changes
- no workflow edits

## Gate expectations

- docs+script-only diff vs `BASE_REF`
- workflow-ban (`.github/workflows/*`)
- rerun Step2 facade/module boundary invariants
- `cargo fmt --check`
- `cargo clippy -p assay-cli --all-targets -- -D warnings`
- replay coverage contract tests:
  - `cargo test -p assay-cli coverage_contract`
  - `cargo test -p assay-cli coverage_out_md`
  - `cargo test -p assay-cli coverage_declared_tools_file`

## Definition of done

- Stacked check passes:
  - `BASE_REF=origin/codex/wave19-coverage-command-step2-mechanical bash scripts/ci/review-coverage-command-step3.sh`
- Promote sanity pass (after sync, if needed):
  - `BASE_REF=origin/main bash scripts/ci/review-coverage-command-step3.sh`
- Step3 diff contains only the 3 allowlisted files
