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

## Semver crate set (derived)

Wave 0 semver checks do not use a hand-maintained crate list. The job derives
its check set from `cargo metadata` via
`scripts/ci/derive-semver-published-lib-crates.py`.

A crate is in the derived set when all are true:

- it is a workspace member;
- it is a local workspace package (`source == null`);
- it is publishable (`publish != []`, including `publish` unset);
- it has a `lib` target.

The semver job itself still runs conditionally (`semver_relevant == true`).
That trigger flips true on:

- workspace-global semver inputs (`Cargo.toml`, `Cargo.lock`);
- any changed path under the manifest directory of a crate in the derived set.

Non-claim (current Wave 0 scope): a change isolated to a `publish = false`
workspace crate does not by itself trigger `semver_relevant`, even if that
crate is a dependency of a derived published library crate. This keeps routing
tied to one derived direct-membership rule and avoids a second dependency-closure
rule in trigger logic.

Non-claim (routing, current scope): changes to semver gate wiring files
themselves do not set `semver_relevant`; they are covered by the semver routing
and semver gate contract tests in CI.

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

## Semver result propagation

Semver checks for public crates execute via the reusable workflow
`.github/workflows/semver-public.yml`, which is called by `.github/workflows/ci.yml`.
Because GitHub Actions reports a caller job (`needs.semver.result`) as successful
even if inner jobs are skipped, the `CI` rollup in `ci.yml` imports and evaluates
both the change detection decision (`semver_relevant`) and the actual child check
conclusion (`semver_public_result`). When semver is relevant (`semver_relevant == 'true'`),
`semver_public_result` must be `success` (or `failure` waived by an explicit, recorded
override); an inner check that skipped or went missing fails closed.

## Stabilization acceptance

Before declaring Wave 0 stable:

1. No new semver false-positive failures across 3 non-refactor PRs.
2. Runtime stays within budget targets above.
