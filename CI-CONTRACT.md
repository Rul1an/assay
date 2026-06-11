# CI Contract: Rul1an/assay

Draft status: review contract before workflow implementation.

`Rul1an/assay` is the public source, release, evidence, runner, and MCP proxy
repository for Assay. Its CI contract must protect three things at once:

- ordinary Rust, Python, docs, and action quality;
- release and provenance integrity for crates, PyPI, binaries, and generated
  artifacts;
- evidence-honesty boundaries around runtime observation, MCP proxy enforcement,
  receipts, Trust Basis output, and public docs.

This contract is a diff from today's repository state. The first rule is no
required-coverage regressions: existing useful gates should stay required or
visible unless this file is updated with a clear replacement.

## 0. As-Is Inventory

Repository state observed on 2026-06-11:

- Visibility: public.
- Default branch: `main`.
- Primary languages: Rust, Python, TypeScript/JavaScript, shell.
- Rust workspace root: `Cargo.toml`, `Cargo.lock`, `rust-toolchain.toml`.
- Core CI workflows:
  - `.github/workflows/ci.yml` (`CI`)
  - `.github/workflows/kernel-matrix.yml` (`Kernel Matrix CI`)
  - `.github/workflows/runner-health.yml` (`Runner Health`)
  - `.github/workflows/smoke-install.yml` (`Smoke Install (E2E)`)
  - `.github/workflows/parity.yml` (`Parity Tests`)
  - `.github/workflows/assay-security.yml` (`MCP Security (Assay)`)
  - `.github/workflows/workflow-security.yml` (`Workflow Security (zizmor)`)
  - `.github/workflows/assay-runner-lane-check.yml`
    (`Assay-Runner Lane Check`)
  - `.github/workflows/split-wave0-gates.yml` (`Split Wave 0 Gates`)
- Experiment, docs, perf, and nightly workflows exist separately and should not
  be promoted into required PR cost by default.
- Release workflow: `.github/workflows/release.yml`, with binary builds,
  release creation, SBOM generation, build provenance attestation, crates.io
  publish through trusted publishing, and PyPI publish through trusted
  publishing.
- Existing posture already present:
  - workflow-level `permissions: {}` in several high-value workflows;
  - explicit per-job permissions;
  - widespread `persist-credentials: false`;
  - third-party actions mostly pinned by commit SHA with version comments;
  - CODEOWNERS for workflows, release, `assay-action/`, `infra/`, and security
    policy/config paths;
  - Dependabot for GitHub Actions, Cargo, and the Python SDK;
  - actionlint configuration for the self-hosted runner labels;
  - CI scope detection with internal skip decisions instead of broad trigger
    skips;
  - kernel/eBPF gating that avoids self-hosted runner work unless relevant;
  - delegated runner lane proof for selected runner-sensitive PRs.
- Live branch protection on `main`, observed through GitHub on 2026-06-11,
  requires `CI`, `lane-check`, and `host-capability-check` with strict
  up-to-date status checks enabled. `docs/BRANCH-PROTECTION-SETUP.md` should
  stay reconciled with that live state.
- Existing public docs and evidence artifacts include guides, references,
  receipt schemas, MCP proxy docs, runner fixtures, experiment reports, JUnit,
  SARIF, Trust Basis output, and compressed evidence examples.

No target workflow should downgrade this inventory unless the contract is
updated with an explicit rationale.

## 1. Required PR Checks

Required PR checks must be cheap enough to stay reliable, broad enough to catch
cross-crate drift, and honest about what they do not observe. Do not make a
required check path-filter out of existence at the workflow trigger. If a check
is required by branch protection, the workflow should start and skip heavy work
inside jobs with an explicit summary.

### CI

Keep `CI` as the universal merge gate unless branch protection is intentionally
changed. It should continue to cover:

- dependency security through `cargo-deny` and `cargo-audit`;
- clippy with warnings denied;
- public/private boundary vocabulary guard;
- publish-shape guardrails for public crates;
- vendored pack sync;
- MCP registry/package checks;
- TypeScript checks where action or package surfaces are involved;
- performance sentinel behavior that is explicitly bounded and summarized;
- workspace tests across Linux/macOS/Windows with the current exclusions and
  feature-aware commands;
- eBPF smoke only when relevant paths require it.

Do not collapse public crate policy, publish-shape, registry, or boundary guards
into release-only checks. Their value is catching tag-time failures before a tag
exists.

### Workflow And Action Security

Keep workflow security visible on workflow/action/dependabot changes and weekly
drift. Add or confirm in implementation PRs:

- `actionlint` for every workflow file.
- `shellcheck` for workflow shell blocks and helper scripts that are executed
  by workflows.
- `zizmor` stays pinned and runs in trusted contexts that can upload SARIF
  safely.
- Every job has `timeout-minutes`, including docs, demo, perf, and nightly
  workflows.
- Workflow-level default remains `permissions: {}` where possible; jobs request
  only the permissions they need.
- `contents: write`, `actions: write`, `security-events: write`,
  `id-token: write`, and `attestations: write` are treated as high-trust
  permissions and require a short comment or environment boundary.
- `pull_request_target` remains absent unless a future PR adds a dedicated
  threat model and never checks out untrusted head code before trust decisions.

Action pinning contract:

- High-trust actions that can affect release artifacts, provenance, SBOM,
  security-tab output, dependency update PRs, or workflow-generated commits
  must use commit-SHA pins with version comments.
- Ordinary setup/check actions should also stay SHA-pinned where practical.
- Owned floating major tags are acceptable only for self-test or compatibility
  checks that do not publish, attest, release, or write repository state.

### Runner And Kernel Gates

Keep the runner-sensitive lanes separate from ordinary PR cost:

- `Kernel Matrix CI` should remain visible for kernel/eBPF/monitor/evidence
  paths and use explicit internal skip summaries when not applicable.
- Self-hosted runner jobs must not run on untrusted fork code.
- The delegated runner lane proof remains the escalation mechanism for PRs that
  need privileged runner evidence before merge.
- `Runner Health` stays scheduled/manual and should not become a required PR
  check.
- Any queue-management workflow with `actions: write` must stay scheduled/manual
  or otherwise restricted to trusted events.

Do not make eBPF, LSM, or host-capability tests imply live provider verification
or Trust Basis truth. They prove the local runner capability and observed
effects for their specific run boundary only.

### Public Artifact Sanitization Guard

Add a required fail-closed public-artifact sanitization check for the whole
repository, excluding only generated, vendored, build, dependency, and cache
output explicitly allowlisted by the implementation. This is a public repo; any
source file, fixture, doc, test, code comment, sample, workflow, generated
example, or release note can leak public text.

Hard rule:

- The sensitive vocabulary list must not be present in this public repository
  and must not be printed in CI logs.

Acceptable implementation patterns:

- Compare normalized tokens or n-grams against HMAC-SHA256 entries supplied from
  a private source plus a separate private HMAC key.
- Run plaintext sensitive-list checks only in trusted private contexts where
  logs are not public and untrusted PR code cannot read the list.
- On fork PRs, run only the public-safe structural portion.

Required-gate split:

- The public-safe structural portion is required on every PR, including forks.
- The full HMAC-denylist comparison runs only for trusted same-repository PRs,
  pushes, and scheduled checks that can access the private source and HMAC key
  safely.
- A degraded fork run must say that private-list comparison was skipped without
  exposing the list, while still enforcing structural public-artifact rules.
- The trusted HMAC-list layer is part of the sanitizer workflow, not a
  separate required context, until a future context-capture/import review says
  otherwise.
- When the trusted HMAC-list layer runs, the list must include the digest for
  the committed public canary fixture. The scanner fails closed on a canary
  miss so key encoding, normalization, or generator drift cannot silently turn
  the trusted layer into a no-op.
- The trusted list must enumerate every spelling, casing, and spacing variant of
  a term. Normalization lowercases, splits on non-alphanumerics, and HMACs
  one-to-five-token windows per line, so a compound spelling and a spaced or
  hyphenated spelling of the same term produce different digests. Variant
  completeness is a property of the trusted list, not the scanner.

Logging contract:

- Report only counts and locations, for example `3 matches in README.md:42`.
- Never print matched text.
- Never print the sensitive term, phrase, unhashed denylist entry, digest, or
  HMAC key.
- Treat printing the matched term as a CI bug and a sanitization failure.

The guard is a backstop, not a guarantee. Human public-artifact review remains
primary because fixed matchers miss variants, spacing, morphology, and context.

Relationship to the existing public/private boundary guard:

- The existing CI boundary guard protects repository policy vocabulary and
  product-boundary drift in source-visible surfaces.
- The public-artifact sanitization guard is broader: it treats every
  non-generated public file or emitted public artifact as a publication
  candidate and prevents private vocabulary from leaving the repo boundary.
- If both guards inspect the same file, either one may fail the PR; neither
  should suppress or downgrade the other.

### Claims And Evidence Boundary Guard

Add an advisory, non-required claims-boundary check for public wording in
tracked prose. This guard is regression prevention, not a cleanup lane: the
initial implementation must run with zero findings on the current public
corpus before it ships.

The guard is deliberately narrow. It matches only curated affirmative
constructions where weak evidence is equated with strong security, trust, or
compliance claims. It must not flag isolated words such as `proof`, `secure`,
`compliance`, or `green`.

Initial rule registry:

- `gate-as-truth`: CI, gate, check, scan, badge, green, or passing status
  phrased as proving, guaranteeing, ensuring, or meaning safety, security,
  compliance, trust, production readiness, or truth.
- `proof-of-x`: affirmative `proof of` or `guarantee of` compliance, security,
  safety, or trust.
- `tool-guarantees`: Assay, this action, a gate, or a check phrased as proving,
  guaranteeing, or ensuring that an agent, tool, or call is safe, secure,
  compliant, or trusted.

The guard must stay negation-aware. Bounded forms such as "not a proof of
compliance", "records the decision, not proof of the effect", and "asserted,
not verified" are examples of the discipline working and must not fail the
check. A reviewed exception may use an inline
`claims-guard: allow: <reason>` marker; the reason is required so the exception
is auditable.

Unlike the private-vocabulary sanitizer, this check may print the flagged
sentence because the text is public and authors need the sentence to repair the
claim. It must print the rule name and a bounded-rephrase suggestion. Its rule
list is public and checked into the repository; no secret, HMAC, or private
term list is involved.

Scope:

- root `README.md`, `CHANGELOG.md`, and `CI-CONTRACT.md`;
- `docs/**` prose files;
- the action marketplace metadata in `assay-action/action.yml` or
  `assay-action/action.yaml`.

The workflow trigger should also include the checker and workflow files
themselves so self-test and review changes exercise the guard, but code,
tests, fixtures, and private notes are not prose scan targets.

Claims in public prose should:

- name the artifact, event, fixture, or observation shape being checked;
- distinguish producer, consumer, verifier, conformance, and release lanes;
- state whether a check is informational, blocking, deny-only, or forwarding;
- state degradation/non-claim when evidence cannot support a stronger statement.

Docs and examples must not imply that Assay:

- verifies live provider behavior unless a run actually observes it;
- turns local runner support into global Trust Basis truth;
- treats a received receipt as trusted without signature/key/anchor context;
- treats keyless conformance as issuer trust;
- treats MCP proxy deny-only modes as allow/forwarding enforcement;
- treats declared manifests as provider-verified grants;
- treats OTel/SARIF/JUnit projections as durable evidence records.

For MCP proxy enforcement specifically:

- manifest-observation mode observes `tools/list` and must not forward
  `tools/call`;
- deny-only enforcing modes must keep `tools/call` fail-closed until the
  explicit allow/forward path exists;
- credential-scope gates are operator configuration unless a later PR adds
  provider-verified grant evidence;
- every forwarding-mode PR needs confused-deputy, drift, caller identity,
  credential scope, and declared/current-manifest gates in the PR description.

Do not add this check to branch protection until it has run quietly across many
ordinary PRs and the live check name is captured in the ruleset contract.

## 2. Scheduled Checks

Scheduled checks are for drift, ecosystem security, and lower-frequency
confidence. They should not become required PR checks unless their failure mode
is cheap, stable, and actionable.

Keep or add:

- weekly workflow security drift (`zizmor`);
- OpenSSF Scorecard as a scheduled advisory. The first implementation uses the
  default `GITHUB_TOKEN`, which can read repository rulesets but may not fully
  measure classic branch-protection or webhook settings unless a future
  read/admin token is intentionally added.
- OSV-Scanner for non-Rust dependency surfaces with resolved manifests or
  lockfiles. RustSec remains owned by `cargo-deny` and `cargo-audit`, including
  any deliberately documented advisory ignores, so scheduled OSV must not
  resurface `Cargo.lock` with a different verdict unless an `osv-scanner.toml`
  mirrors the same Rust policy. The explicit non-Rust target list must fail
  closed when a listed target is removed, or when a new tracked
  `package-lock.json` or `requirements*.txt` appears outside the list.
- CodeQL/default code scanning for Rust-adjacent glue where available, Python,
  JavaScript/TypeScript, shell, and workflow files;
- ClusterFuzzLite only for small deterministic parsers/canonicalizers and
  evidence-reference surfaces, with bounded corpora and timeouts;
- dependency-review or equivalent for PR dependency deltas where GitHub Advanced
  Security is available;
- scheduled release-smoke rehearsal that does not publish;
- scheduled MCP proxy fixture replay with no live providers;
- scheduled public-artifact sanitization trusted run with the full private HMAC
  source;
- scheduled stale evidence-artifact inventory for compressed fixtures, large
  generated docs assets, and run output.

The scheduled supply-chain posture workflows are advisory only. They run on a
weekly cadence plus manual dispatch, do not run on ordinary pull requests, do
not mutate rulesets, and must not be added as required contexts without a
separate context-capture review. Advisory vulnerability findings should not
fail the scheduled OSV job, but scanner execution failures, malformed output,
or target-list coverage drift should fail loudly.

Do not schedule by default:

- live model/provider runs;
- broad OS/feature matrices without a measured failure class;
- privileged self-hosted runner jobs that can be replaced by a manual delegated
  proof;
- network-dependent smoke tests that cannot distinguish dependency outage from
  regression.

## 3. Release-Only Checks

Release checks protect artifacts, publication, and provenance. Keep them stricter
than ordinary PR checks.

Keep:

- tag/manual release version validation;
- cross-platform binary builds and checksums;
- MCP server binary builds;
- SBOM generation;
- build provenance attestation;
- crates.io trusted publishing;
- PyPI trusted publishing;
- environment protection for release, crates, and PyPI publication;
- release notes and public docs boundary checks before publishing;
- post-release smoke/install validation.

Add or confirm:

- release artifact manifest verifies every uploaded archive, checksum, SBOM, and
  attestation subject.
- `gh release` calls use explicit file lists, not broad globs that can pick up
  stale artifacts.
- release jobs do not reuse untrusted PR artifacts.
- release dry-run/rehearsal remains non-publishing.
- any release job with `id-token: write`, `attestations: write`, or
  `contents: write` has a dedicated job boundary and environment gate.

Current release boundary:

- Attestations bind the workflow identity and release artifact subjects.
- They do not prove runtime truth, live provider behavior, issuer trust for
  external receipts, or correctness beyond the artifact and workflow boundary.

## 4. Manual Checks

Manual workflows are acceptable for checks that require judgement, elevated
runtime cost, privileged runner state, or release rehearsal:

- delegated runner proof;
- host capability proof;
- monitor attach smoke;
- runner OTel experiments;
- performance forensics;
- ADR/nightly historical experiments;
- release dry runs;
- MCP proxy enforcement staging runs;
- expanded fuzz/corpus runs.

Manual workflow requirements:

- Inputs must be single-line or structured-validated before use.
- Downloaded release assets must be checksum-verified.
- Any generated public artifact must pass sanitization and claims-boundary
  checks before publication.
- Manual runs must say whether their output is evidence, fixture, diagnostic
  metadata, or operational telemetry.

## 5. Non-Goals And Non-Claims

Non-goals:

- No live provider dependency in required PR checks.
- No self-hosted runner dependency for ordinary PRs.
- No broad matrix expansion without a measured failure class.
- No path-filtered required check that can remain pending forever.
- No release/publish authority in fork PRs.
- No forwarding-mode MCP enforcement without explicit caller, credential,
  manifest, drift, and confused-deputy gates.
- No second evidence semantics layer outside the documented Assay artifact
  contracts.

Allowed language:

- "Observed runtime evidence."
- "Deny-only MCP proxy mode."
- "Conformance check."
- "Signature verification when signer/key context is supplied."
- "Attestation over release artifact subjects."
- "Operational telemetry."
- "Projection to SARIF/JUnit/OTel."

Disallowed without explicit boundary:

- Claims that Assay verifies all provider behavior.
- Claims that a local capability proof upgrades every future run.
- Claims that a digest-only or keyless check proves issuer trust.
- Claims that receipt arrival over an authorized channel is enough.
- Claims that OTel/SARIF/JUnit output is the durable evidence record.
- Claims that a deny-only proxy path forwards safely.
- Claims that release attestation proves runtime behavior.

The sanitization guard is separate from these claim-boundary rules. It protects
private strategy vocabulary from appearing in public artifacts and must do so
without reprinting protected vocabulary.

## 6. Required Context Names

Branch protection is enforced by exact check context names, not by this file.
Before making any branch-protection changes:

1. Open a draft PR that implements the workflows.
2. Query the live check runs for that PR.
3. Copy the exact check names into this section.
4. Treat future job renames as breaking changes because they can silently
   un-gate protected branches.

Currently required live branch-protection contexts:

- `CI`
- `lane-check`
- `host-capability-check`

Context groups that should stay visible and reviewed when relevant:

- `Workflow Security (zizmor)`
- `MCP Security (Assay)`
- `Kernel Matrix CI`
- `Runner Health`
- `Smoke Install (E2E)`
- `Parity Tests`
- `assay-action-contract-tests`
- `Split Wave 0 Gates`

Observed from the CI baseline implementation PR `#1638`:

- `public-artifact-sanitization`

Proposed required context names for the next branch-protection review:

- `CI`
- `lane-check`
- `host-capability-check`
- `public-artifact-sanitization`

Checked-in ruleset activation lives at
`.github/rulesets/main-required-ci-contexts.json`.

Import note: the checked-in ruleset is config-as-code only until imported in
GitHub settings. It intentionally mirrors the current live branch protection for
`CI`, `lane-check`, and `host-capability-check`, so importing it does not weaken
the existing required-check set. Add `bypass_actors` only if the repository
owner intentionally wants to preserve an admin bypass path; otherwise
`strict_required_status_checks_policy: true` means merges must be rebased-current
and green.

Do not make these required in this slice:

- Manual, release-only, scheduled-only, or host-capability-specific jobs.
- Matrix leaf jobs that are already summarized by a stable required gate.
- External advisory checks unless the repository owner intentionally accepts
  their availability as a merge dependency.

Do not require a workflow by branch protection if that workflow uses top-level
path filters that can skip the run entirely. Either keep it advisory or make it
always start and skip internally.

Future Scorecard, OSV, CodeQL, ClusterFuzzLite, or claims-boundary jobs should
be added here only after their live PR check names are observed.

## 7. Target Workflow Files

Expected target workflow set:

- `.github/workflows/ci.yml` kept and tightened, not removed.
- `.github/workflows/workflow-security.yml` kept.
- `.github/workflows/kernel-matrix.yml` kept with internal skip summaries.
- `.github/workflows/assay-runner-lane-check.yml` kept.
- `.github/workflows/release.yml` kept with high-trust release boundaries.
- `.github/workflows/sanitize-public-artifacts.yml` for public artifact
  sanitization, unless folded into `ci.yml`.
- `.github/workflows/claims-boundary.yml` for public wording and non-claim
  checks, unless folded into sanitization.
- `.github/workflows/osv-scorecard.yml` for scheduled OSV/Scorecard if GitHub
  default/security-tab coverage is not enough.
- `.github/workflows/codeql.yml` only if GitHub default setup is not enabled or
  cannot cover the relevant languages.
- `.github/workflows/clusterfuzzlite.yml` only if bounded deterministic fuzz
  targets are selected and reviewed.

Implementation should happen in small follow-up PRs after this contract is
reviewed.
