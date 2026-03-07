# ACP Step3 Checklist (Closure)

Scope lock:
- `docs/contributing/SPLIT-CHECKLIST-acp-step3.md`
- `docs/contributing/SPLIT-REVIEW-PACK-acp-step3.md`
- `scripts/ci/review-acp-step3.sh`
- no code changes in Step3
- no workflow changes

## Closure invariants

- re-run Step2 quality checks (`fmt`, `clippy`, ACP tests)
- re-run Step2 facade invariants (`lib.rs` thin wrapper)
- re-run Step2 visibility invariants (`adapter_impl/*` is `pub(crate)` only)
- re-run must-survive ACP test-name markers

## Gate requirements

- allowlist-only diff vs Step2 base branch
- workflow-ban (`.github/workflows/*`)
- quality checks:
  - `cargo fmt --check`
  - `cargo clippy -p assay-adapter-acp -p assay-adapter-api --all-targets -- -D warnings`
  - `cargo test -p assay-adapter-acp`

## Definition of done

- `BASE_REF=origin/codex/wave14-acp-step2-mechanical bash scripts/ci/review-acp-step3.sh` passes
- Step3 diff is docs+script only
