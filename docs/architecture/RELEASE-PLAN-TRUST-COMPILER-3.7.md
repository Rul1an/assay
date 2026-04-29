# Release Plan — Trust Compiler 3.7 Evidence Portability

**Purpose:** Record **v3.7.0** as the first public Assay line where three
bounded receipt families are claim-visible in Trust Basis: eval outcomes,
runtime decision details, and inventory/provenance surfaces.

**Status:** released as **v3.7.0** on 2026-04-29.

The release workflow created the GitHub Release, release assets, provenance
artifacts, crates.io publications, and PyPI publication from tag **`v3.7.0`**.

**SSOT (do not re-invent semantics here):**

- [CHANGELOG.md](../../CHANGELOG.md) — factual shipped items for the release.
- [Receipt family matrix](../reference/receipt-family-matrix.json) — receipt
  families, event types, included fields, excluded fields, and non-claims.
- [PLAN-P41](PLAN-P41-OPENFEATURE-EVALUATION-DETAILS-DECISION-RECEIPT-IMPORT-2026q2.md) — OpenFeature boolean decision receipt import boundary.
- [PLAN-P43](PLAN-P43-CYCLONEDX-MLBOM-MODEL-COMPONENT-RECEIPT-IMPORT-2026q2.md) — CycloneDX ML-BOM model-component receipt import boundary.
- [PLAN-P45](PLAN-P45-INVENTORY-RECEIPT-TRUST-BASIS-CLAIM-2026q2.md) — bounded Trust Basis claim for supported inventory receipt boundaries.
- [PLAN-P45b](PLAN-P45B-DECISION-RECEIPT-TRUST-BASIS-CLAIM-2026q2.md) — bounded Trust Basis claim for supported decision receipt boundaries.

The companion Harness recipe lives in `Rul1an/Assay-Harness`. Assay owns the
artifact semantics; Assay Harness owns CI orchestration and review projection.

---

## Framing

**Suggested lead:**

> Assay v3.7.0 completes the first three-family evidence-portability line:
> selected eval outcomes, runtime decision details, and model
> inventory/provenance surfaces can be reduced into bounded Assay receipts,
> bundled, compiled into Trust Basis, and compared as claim-level artifacts.

Avoid calling this Promptfoo, OpenFeature, CycloneDX, or Mastra integration,
partnership, official support, correctness, safety, or compliance truth. These
are downstream receipt compiler lanes over existing public surfaces.

---

## Version Advice: 3.7.0 Minor

This is a minor release, not a patch:

| Area | Why it fits a minor bump |
|------|--------------------------|
| CLI surface | Adds supported receipt importers beyond Promptfoo: OpenFeature decision details, CycloneDX ML-BOM model components, and Mastra ScoreEvent artifacts. |
| Trust Basis | Adds additive visibility for supported decision and inventory receipt boundaries. |
| Trust Card | Bumps the visible Trust Card schema to v5 because the claim table changes. |
| Consumer story | Moves evidence portability from one eval lane to a three-family claim-visible surface. |

Patch releases remain for fixes-only. This release changes the user-visible
artifact pipeline while keeping the epistemic boundary narrow.

---

## Pre-Release Verification

- [x] `cargo fmt --check`
- [x] `cargo check -p assay-cli --all-targets`
- [x] `cargo test -p assay-evidence trust_basis -- --nocapture`
- [x] `cargo test -p assay-cli --test evidence_test -- --nocapture`
- [x] `cargo test -p assay-cli --test trust_basis_test -- --nocapture`
- [x] `cargo test -p assay-cli --test trustcard_test -- --nocapture`
- [x] `cargo run -p assay-cli -- evidence import openfeature-details --help`
- [x] `cargo run -p assay-cli -- evidence import cyclonedx-mlbom-model --help`
- [x] `cargo run -p assay-cli -- evidence import mastra-score-event --help`
- [x] crates.io publication completed through the release workflow.
- [x] [CHANGELOG.md](../../CHANGELOG.md), [README.md](../../README.md), and
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

- Import bounded OpenFeature boolean `EvaluationDetails` rows into verifiable
  decision receipt bundles with `assay evidence import openfeature-details`.
- Import one selected CycloneDX ML-BOM `machine-learning-model` component into
  inventory receipt bundles with `assay evidence import cyclonedx-mlbom-model`.
- Import reduced, reviewer-safe Mastra ScoreEvent JSONL artifacts with
  `assay evidence import mastra-score-event`.
- Compile supported receipt bundles into Trust Basis and compare Trust Basis
  artifacts with `assay trust-basis diff`.

### Integrators

- Preserve the bounded-receipt distinction: raw upstream payloads, provider
  config, targeting context, full BOM graphs, model-card bodies, dataset bodies,
  and observability trace trees remain outside the receipt truth boundary.
- Treat `external_eval_receipt_boundary_visible`,
  `external_decision_receipt_boundary_visible`, and
  `external_inventory_receipt_boundary_visible` as boundary/provenance claims,
  not correctness or compliance claims.
- Key Trust Basis and Trust Card consumers by stable `claim.id`, not row count.

### What This Release Is Not

- Not official integration, endorsement, or partnership with any upstream tool.
- Not full Promptfoo, OpenFeature, CycloneDX, or Mastra export support.
- Not model correctness, flag correctness, model safety, dataset approval, BOM
  completeness, license, vulnerability, or compliance truth.
- Not a new aggregate trust score or `safe/unsafe` badge.
- Not a Trust Basis score claim for Mastra ScoreEvent receipts.

---

## Companion Harness Release

Prepare an Assay Harness **v0.3.0** companion release after the Assay tag. The
Harness release should carry the Promptfoo, OpenFeature, and CycloneDX recipes
over the Assay v3.7.0 Trust Basis surface. Use a Harness-local version line; do
not reuse Assay's `3.7.0` semver.

---

## Tagging

After all release checks pass on the merged release commit:

```bash
git tag v3.7.0
git push origin v3.7.0
```

The tag runs [`.github/workflows/release.yml`](../../.github/workflows/release.yml).
Crates.io publishing should be confirmed separately with dry-runs and the
release workflow outcome.
