# Wave 0 Gates

Operational notes for `.github/workflows/split-wave0-gates.yml`.

## Scope

Wave 0 gates are the pre-refactor guardrails for:

- feature drift
- semver drift for public crates
- placeholder/temporary panic regressions
- unsafe-boundary creep (warn-only in first iteration)

## Baseline SHA policy (semver checks)

- Source of truth: workflow env `WAVE0_SEMVER_BASELINE_SHA`.
- Current pinned baseline: `9cc23b4c684be7cfd81f170c4f66d59903dd76eb`.
- Reset cadence: update once at the start of a refactor sprint, not during a sprint.
- Update rule: change SHA + mention the reset in PR description with reason.

## Runtime budget targets

- `feature-matrix` job: target <= 25 minutes on `ubuntu-latest`.
- `semver-public` job: target <= 15 minutes on `ubuntu-latest`.
- Total Wave 0 workflow target: <= 40 minutes.

If budget is exceeded:

1. Keep curated feature sets blocking.
2. Move expensive exploratory checks to non-blocking/nightly lanes.
3. Keep `cargo-hack` conditional on touched crates only.

## Cargo-hack policy

- `cargo-hack` is conditional and runs only for touched hotspot crates.
- Current hotspot crates: `assay-core`, `assay-cli`, `assay-registry`.
- `assay-cli` excludes `experimental` in blocking lane:
  - `cargo hack check -p assay-cli --each-feature --exclude-features experimental`

## Semver allowlist (public crates)

Wave 0 semver gate runs on the library-API subset of the current public
crates.io contract:

- `assay-common`
- `assay-policy`
- `assay-metrics`
- `assay-core`
- `assay-registry`
- `assay-evidence`

Checks are still conditional on touched/global change detection.

The full current crates.io publish contract is enforced separately by
`scripts/ci/check-public-crate-policy.sh` and `scripts/ci/publish_idempotent.sh`.
Binary- or operational-facing crates such as `assay-cli`, `assay-monitor`,
`assay-mcp-server`, and `assay-sim` are published, but are not part of this
Wave 0 library semver allowlist unless a future gate slice adds stable library
API coverage for them.

The Assay-Runner substrate crates — `assay-runner-schema`,
`assay-runner-core`, and `assay-runner-linux` — are also published as of
`v3.11.3`, but their package descriptions explicitly
frame them as internal/experimental substrate (no standalone product
guarantee, intentionally undocumented for third-party use, semver tracks
the Assay workspace). They are intentionally **not** in the Wave 0 library
semver allowlist; they exist on crates.io only because `assay-cli` depends
on them and cargo publish requires every declared dep to be resolvable
from crates.io.

As of `v3.11.3`, `check-public-crate-policy.sh` also runs as a PR-CI
guardrail (job `Public crate policy` in `ci.yml`), so the policy gate
fires before tag, not at release time.

## Nightly safety lane (Wave 0.1)

- Current status: non-blocking stub job in Wave 0 workflow (`continue-on-error: true`).
- Next increment (Wave 0.1):
  - focused `cargo miri test` targets
  - parser/crypto fuzz smoke with fixed runtime budget
  - optional Kani lane (opt-in)

## Required checks recommendation

Configure branch protection to require:

- `Wave 0 feature matrix`
- `Wave 0 quality gates`
- `Wave 0 semver checks (public crates)`

Wave 0 workflow always triggers on `pull_request`; heavy jobs are conditional to avoid docs-only blocking.

## Stabilization acceptance

Before declaring Wave 0 stable:

1. No new semver false-positive failures across 3 non-refactor PRs.
2. Runtime stays within budget targets above.
3. Unsafe preview remains non-blocking until monitor split isolates unsafe code.
