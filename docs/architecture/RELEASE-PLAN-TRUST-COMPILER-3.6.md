# Release Plan — Trust Compiler 3.6 Evidence Portability

**Purpose:** Prepare **v3.6.0** as the first public Assay line where selected
external evaluation outcomes can enter the trust-compiler path as bounded
evidence receipts.

**Status:** workspace and docs are prepared for **v3.6.0**. Tag
**`v3.6.0`** only after the release-prep PR is merged and the pre-release checks
below pass on the release commit. Crates.io publication remains a separate
publish step after the tag/release workflow.

**SSOT (do not re-invent semantics here):**

- [CHANGELOG.md](../../CHANGELOG.md) — factual shipped items for the release.
- [PLAN-P31](PLAN-P31-PROMPTFOO-JSONL-COMPONENT-RESULT-RECEIPT-IMPORT-2026q2.md) — Promptfoo JSONL component result receipt import boundary.
- [PLAN-P33](PLAN-P33-EXTERNAL-EVAL-RECEIPT-TRUST-BASIS-CLAIM-2026q2.md) — bounded Trust Basis claim for supported external eval receipt boundaries.
- [PLAN-P34](PLAN-P34-TRUST-BASIS-DIFF-GATE-2026q2.md) — Trust Basis diff contract and regression semantics.
- [From Promptfoo JSONL to Evidence Receipts](../notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md) — evidence-portability note.

The companion Harness recipe lives in `Rul1an/Assay-Harness`. Assay owns the
artifact semantics; Assay Harness owns CI orchestration and review projection.

---

## Framing

**Suggested lead:**

> Assay v3.6.0 adds the first external-eval evidence portability lane. Selected
> Promptfoo assertion component results can be reduced into bounded Assay
> evidence receipts, bundled, compiled into Trust Basis, and compared as
> claim-level artifacts without importing full eval-run truth or claiming model
> correctness.

Avoid calling this a Promptfoo integration, partnership, official support line,
or eval-run correctness feature. Promptfoo remains the CI/eval runner; Assay is
the evidence layer for selected outcomes.

---

## Version Advice: 3.6.0 Minor

This is a minor release, not a patch:

| Area | Why it fits a minor bump |
|------|--------------------------|
| CLI surface | Adds `assay evidence import promptfoo-jsonl` for supported external evidence import. |
| Trust Basis | Adds additive visibility for supported external eval receipt boundaries. |
| Diff contract | Adds `assay trust-basis diff` for claim-level regression comparison. |
| Consumer story | Adds a new downstream evidence-portability path that CI consumers can build around. |

Patch releases remain for fixes-only. This release changes the user-visible
artifact pipeline while keeping the epistemic boundary narrow.

---

## Pre-Release Verification

- [ ] P40 public-surface sync is merged before this release-prep PR is merged or
  this PR is retargeted to `main`.
- [ ] `cargo fmt --check`
- [ ] `cargo check -p assay-cli --all-targets`
- [ ] `cargo test -p assay-cli promptfoo_jsonl -- --nocapture`
- [ ] `cargo run -p assay-cli -- evidence import promptfoo-jsonl --help`
- [ ] `cargo run -p assay-cli -- trust-basis diff --help`
- [ ] `cargo publish -p assay-cli --dry-run` or a documented crates.io blocker.
- [ ] [CHANGELOG.md](../../CHANGELOG.md), [README.md](../../README.md), and
  [docs/ROADMAP.md](../ROADMAP.md) agree on the release line.

Publish-order note: `assay-cli` depends on internal workspace crates at the same
version. A pre-publish `assay-cli` dry-run can fail until `assay-common` and the
other dependency crates are visible on crates.io. That is a publish-order
blocker, not an `assay-cli` packaging failure. The release workflow publishes
the configured crate list in dependency order via
[`scripts/ci/publish_idempotent.sh`](../../scripts/ci/publish_idempotent.sh).

---

## Release Notes Outline

### Users

- Import selected Promptfoo assertion component results into verifiable Assay
  evidence bundles with `assay evidence import promptfoo-jsonl`.
- Compile those bundles into Trust Basis artifacts and compare Trust Basis
  artifacts with `assay trust-basis diff`.
- Keep treating Trust Basis and Trust Card claims as claim rows keyed by stable
  `claim.id`, not row count or ordering.

### Integrators

- The Promptfoo lane is strict in v1: Promptfoo CLI JSONL rows,
  `gradingResult.componentResults[]`, `equals` assertions, and binary component
  scores.
- Raw prompt, output, expected value, vars, provider payloads, token/cost data,
  and full JSONL rows remain out of scope.
- `external_eval_receipt_boundary_visible` means the bounded receipt boundary is
  visible; it does not mean the upstream eval passed or that the model output was
  correct.

### What This Release Is Not

- Not a Promptfoo integration or partnership claim.
- Not a full Promptfoo export importer.
- Not red-team report support.
- Not a model-correctness, eval-pass, or compliance claim.
- Not a new aggregate trust score or `safe/unsafe` badge.

---

## Companion Harness Release

If the v3.6.0 release notes point users at the runnable Promptfoo receipt
pipeline recipe, prepare an Assay Harness companion release as well. The Harness
release should carry the `trust-basis gate`, `trust-basis report`, contract
fixtures, and P38 recipe as operational CI tooling above the Assay artifact
contracts. Use a Harness-local version line; do not reuse Assay's `3.6.0`
semver.

---

## Tagging

After all release checks pass on the merged release commit:

```bash
git tag v3.6.0
git push origin v3.6.0
```

The tag runs [`.github/workflows/release.yml`](../../.github/workflows/release.yml).
Crates.io publishing should be confirmed separately with dry-runs and the
release workflow outcome.
