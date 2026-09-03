# Wave 0 Gates

Operational notes for `.github/workflows/split-wave0-gates.yml`.

## Scope

Wave 0 gates are the pre-refactor guardrails for:

- feature drift
- semver drift for public crates
- placeholder/temporary panic regressions

## Baseline policy (semver checks)

- Source of truth: the newest `v[0-9]*` release tag, resolved at job time
  (`git tag --list 'v[0-9]*' --sort=-v:refname`).
- There is no pinned baseline SHA. A missing tag fails the job.
- Baseline selection and the real lint's ability to reject a planted API break are pinned by
  `scripts/ci/test-semver-gate.sh`.
- Change detection and the route from a touched crate to its semver invocation are pinned by
  `scripts/ci/test-split-wave0-semver-routing.sh`.

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
- `assay-runner-schema`

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
the Assay workspace). `assay-runner-schema` is the narrow exception in this
allowlist: ADR-048 requires its existing public type paths to survive a move
to shared definitions, so that migration needs a real semver check. This does
not grant a standalone-product guarantee. `assay-runner-core` and
`assay-runner-linux` remain outside the Wave 0 library semver allowlist; the
substrate crates exist on crates.io because `assay-cli` depends on them and
cargo publish requires every declared dependency to be resolvable there.

As of `v3.11.3`, `check-public-crate-policy.sh` also runs as a PR-CI
guardrail (job `Public crate policy` in `ci.yml`), so the policy gate
fires before tag, not at release time.

## Nightly safety lane (Wave 0.1)

- Current status: non-blocking stub job in Wave 0 workflow (`continue-on-error: true`).
- Next increment (Wave 0.1):
  - focused `cargo miri test` targets
  - parser/crypto fuzz smoke with fixed runtime budget
  - optional Kani lane (opt-in)

## Required checks

The live required contexts are named once in `CI-CONTRACT.md` at
`Currently required live branch-protection contexts`, and
`scripts/ci/check-required-contexts.py` pins that list to
`.github/rulesets/main-required-ci-contexts.json`. Do not copy the names here.

Wave 0 job names (`Wave 0 feature matrix`, `Wave 0 quality gates`,
`Wave 0 semver checks (public crates)`) are workflow jobs, not current
required contexts.

Wave 0 workflow always triggers on `pull_request`; heavy jobs are conditional to avoid docs-only blocking.

## Stabilization acceptance

Before declaring Wave 0 stable:

1. No new semver false-positive failures across 3 non-refactor PRs.
2. Runtime stays within budget targets above.
