# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

- Clarified the semantic-gap scenario plan so the `path_rewrite`
  table allows both target-only and link-plus-target archive shapes, and
  so `ambiguous_proximity` is documented only as a freeform diagnostic
  note rather than a join-result enum value.
- Added a CycloneDX ML-BOM formulation fixture that keeps training,
  evaluation, and handoff workflow context in the source BOM while proving the
  importer still emits only the bounded inventory receipt claim.
- Added a plan-only Runner-vs-OTel overhead measurement follow-up that
  fixes the sample sizes, host-boundary rules, BMF-compatible output
  shape, and non-claims required before publishing wall-clock or RSS
  numbers.
- Added the Slice 1 local Arm B overhead harness with
  `assay.experiment.overhead_sample.v0` /
  `assay.experiment.overhead_summary.v0` schema sidecars and tests. The
  harness emits local measurement artifacts but does not commit or
  publish benchmark numbers.
- Added the Slice 2 delegated Arm C overhead workflow for
  `assay-bpf-runner`. The workflow uploads health-gated overhead
  artifacts for review but still does not commit benchmark numbers. BMF
  metric keys now use full arm slugs such as `arm_b_otel` and
  `arm_c_dual_capture` to keep future arms unambiguous.
- Added Slice 3 RSS collection support to the overhead harness. The
  harness can now wrap samples in `/usr/bin/time`, parse GNU time and
  macOS time peak-RSS output, emit `rss-sizes.json`, and include RSS
  metrics in the derived BMF export when present.
- Added the Slice 4 overhead summary renderer. The harness now writes a
  reviewer-friendly `summary.md` beside `summary.json`, and the
  delegated workflow appends that Markdown to the GitHub step summary.
- Added the Slice 5 overhead findings document. The findings summarize
  the clean delegated Arm C host-class baseline and explicitly withhold
  Arm B-vs-Arm C deltas until same-host Arm B measurements land.
- Added a delegated same-host Arm B path to the Runner-vs-OTel overhead
  workflow so `arm-b-otel` can be measured on `assay-bpf-runner` before
  any Arm B-vs-Arm C delta is published.
- Updated the overhead findings after the same-host Arm B dispatches.
  The findings now report the narrow `linux-aarch64-6.8.0-117-generic`
  Arm B-vs-Arm C delta while preserving the non-co-temporal and
  non-decomposition caveats.
- Added optional Arm A runner-only overhead dispatch wiring so the
  current Arm C delta can be decomposed into Runner archive-only cost
  versus Runner archive plus OTel trace cost.
- Updated the overhead findings after the Arm A runner-only dispatches.
  The findings now record the same-host three-arm measurement set and
  classify wall-clock decomposition as inconclusive while showing that
  the observed RSS delta is dominated by Runner capture.
- Tightened the Runner-vs-OTel overhead workflow diagnostics so failed
  harness runs still upload partial measurement artifacts and planned
  the next phase-timing slice for localizing Runner wall-clock overhead.
- Refreshed the Runner-vs-OTel overhead findings with a healthy Arm A
  wall-clock repeat, preserving the conclusion that RSS decomposes
  cleanly while wall-clock does not yet support an additive split.
- Added Runner-vs-OTel overhead phase-timing diagnostics for Arm A/C:
  `assay runner-spike` can now emit an experiment-scoped
  `assay.experiment.runner_phase_timing.v0` side log, and the overhead
  harness aggregates those phases into samples, summaries, Markdown, and
  BMF output without changing Runner archive contracts.
- Updated the overhead findings after the Slice 8 Arm A/C phase-timing
  dispatches. The phase data explains part of the Arm A / Arm C median
  wall-clock gap, mostly around monitor attach, but still withholds an
  additive wall-clock decomposition claim.
- Added Slice 9 paired Arm A/C residual diagnostics planning and
  workflow support. The overhead workflow can now run `arm=paired-a-c`
  as adjacent counterbalanced pairs and emit `paired-sequence.json` with
  per-sample phase residuals for review.
- Updated the overhead findings after the Slice 9 paired Arm A/C
  dispatch. The paired run shows the earlier Arm A-over-Arm C median
  wall-clock gap does not reproduce under adjacent pairing, so the
  wall-clock decomposition remains unpublished and the RSS decomposition
  remains the stable finding.
- Planned Slice 10 of the Runner-vs-OTel overhead follow-up as a
  controlled event-rate / workload-intensity sweep. The next useful
  question is how overhead scales with kernel-event rate, span/event
  rate, concurrency, and payload size, not another broad Arm A/C
  wall-clock rerun.
- Added Slice 10 harness/workflow support for that event-rate sweep. The
  overhead workflow now accepts sweep inputs, the workload can generate
  controlled kernel-event and OTel event pressure, and samples/summaries
  embed `assay.experiment.event_rate_sweep.v0` metadata without
  publishing new measurements.
- Recorded the post-merge Slice 10 smoke dispatches for the event-rate
  sweep. Runs 26508127380 and 26508355816 verified paired Arm A/C sweep
  metadata, kernel-event pressure, Arm C span-event metadata, and clean
  health gates without promoting n=2 smoke runs into benchmark findings.
- Planned the Slice 11 starter matrix for the event-rate sweep:
  predeclared paired A/C control, kernel-high, span-high,
  kernel-concurrent, and corner cells with n=5 per cell and explicit
  event-count, health-gate, and non-publication rules.
- Recorded Slice 11 event-rate starter-matrix findings. All five paired
  A/C cells passed with 5/5 valid samples per arm, clean Runner health
  gates, and calibrated kernel/span event targets; no health boundary was
  reached at 100 kernel events, 100 span events, concurrency 4, and
  64 KiB span payloads.
- Planned Slice 12 as a SOTA-informed boundary-finding sweep. The next
  overhead step is to extend event-rate targets beyond `high=100`, then
  run a small paired A/C widening matrix that reports health/fidelity
  boundaries rather than another broad wall-clock decomposition.
- Added Slice 12 harness support for the boundary-finding sweep:
  `assay.experiment.event_rate_sweep.v0.1` extended `x500` / `x1000`
  targets, optional warm-up samples, and longer delegated workflow
  timeouts. The docs pin warm-up failures as review-artifact diagnostics
  that do not abort the harness but make an all-warm-up-failed dispatch
  inconclusive. This does not dispatch the widening matrix or publish new
  measurement claims.
- Recorded Slice 12 boundary-finding results. The widened paired A/C
  runs kept Runner health clean and kernel-event calibration exact
  through `x1000` / concurrency 16, while widened OTel span-event cells
  hit the default 128-event retention boundary at `s500`, so no timing
  slope is published beyond that span-fidelity limit.
- Verified the Slice 12 span-fidelity mechanism against the
  OpenTelemetry Span Limits default, retained event-index ranges, and a
  local `OTEL_SPAN_EVENT_COUNT_LIMIT=1000` repro before treating the
  128-event cap as an OTel SDK configuration boundary.
- Added a span-event limit guardrail to the overhead harness. Non-baseline
  sweep samples and summaries now record the effective OTel span-event
  limit and warn when `target_span_events` exceeds that limit, so future
  dispatches cannot silently treat clipped span-event counts as throughput
  evidence.
- Added a citation-oriented Runner-vs-OTel overhead findings summary that
  separates the three closed arc results: non-additive wall-clock behavior,
  stable RSS decomposition, and the Runner-kernel / OTel-span fidelity
  boundary.
- Added a SOTA-informed agent-observability fidelity roadmap that turns
  the completed overhead and trace/archive experiments into prioritized
  follow-up slices for calibration guardrails, portable evidence packs,
  semantic-gap scenarios, and OTel/OpenInference interoperability. The
  roadmap now starts with experiment namespace-governance rules for
  naming, promotion, artifact-family inventory, calibration verdicts and
  methods, and evidence-pack minimums.
- Added the first agent-observability fidelity guardrail to the
  Runner-vs-OTel overhead harness. Non-baseline sweep samples and
  summaries now embed
  `assay.experiment.agent_observability_fidelity.calibration.v0` with
  requested-vs-observed kernel/span counts, kernel-layer path matching
  methods, per-layer agreement, and a compact `fidelity_verdict`.
  Fidelity calibration now moves from `proposed` to `experiment-scoped`
  in the artifact-families inventory.
- Added the first agent-observability evidence-pack prototype. The
  experiment-scoped `evidence_pack.py` generator emits a strict v0 pack
  manifest, one-page Markdown summary, observation-health artifact,
  optional trace JSON, Runner archive/reference copy, and explicit
  redaction manifest without promoting evidence packs to a product API.
  Evidence packs now move from `proposed` to `experiment-scoped` in the
  artifact-families inventory.
- Planned the first semantic-gap scenario matrix for the
  agent-observability fidelity roadmap. The plan predeclares a
  deterministic safe-read baseline, five divergence/fallback scenarios,
  join-grade requirements, claim-class rules, evidence-pack output
  expectations, and the minimum Slice 4 harness exit gate without adding
  a harness or dispatching measurements.
- Clarified the semantic-gap pre-harness contract before implementation:
  the `path_rewrite` fixture uses a symlink-resolution pattern, runtime
  side effects remain run-scope or `timestamp_or_order` diagnostic joins
  unless a strong key exists, and Slice 4's MVP gate can be synthetic
  while delegated capture remains required before publishing measured
  findings.
- Added the Slice 4 semantic-gap MVP harness. The synthetic harness
  emits `matched_safe_read`, `hidden_write`, and `weak_join_fallback`
  scenario directories with trace/archive fixtures, join-result rows,
  claim-class cells, bounded semantic-gap verdicts, and evidence packs
  without dispatching delegated runs or publishing semantic-gap
  findings.
- Tightened the semantic-gap MVP harness after review by keeping
  synthetic fixture schema strings under
  `assay.experiment.agent_observability_fidelity.*`, adding schema
  conditional coverage for `inconclusive` verdicts, and pinning the
  scenario-id enum/CLI generation paths in tests.

## [3.12.0] - 2026-05-25

> **Runner evidence and drift-reporting release.**
>
> `v3.12.0` turns the post-`v3.11.3` measured-run work into a release
> line: real Linux/eBPF experiment packages, runtime-drift projection
> reports, schema sidecars, and release-grade provenance around how drift
> reports were rendered. The new surfaces remain low-level and
> evidence-first. They do not introduce new Trust Card claims, policy
> verdicts, or standalone guarantees for the `assay-runner-*` crates.

### Runner-vs-OTel / OpenInference experiment package

- Added the `runner-vs-otel-2026-05` experiment package with a controlled
  three-arm comparison between in-process OTel/OpenInference-style traces
  and out-of-band Runner archives captured with Linux/eBPF + cgroup-v2.
- Recorded real delegated Arm C baselines (`n=3`) with per-run
  tamper-evident manifest binding, clean measurement-health gates
  (`ringbuf_drops=0`, `kernel_layer=complete`,
  `cgroup_correlation=clean`), and explicit non-claims around archive byte
  determinism.
- Added SDK-layer ingestion for tool-level `gen_ai.tool.call.id` joins and
  a controlled tool-call argument tampering scenario where reported intent
  and measured filesystem effect diverge at the same tool call id.
- Added publication drafts and the filed OpenInference vocabulary discussion
  framing for runtime-evidence artifact links. The ask stays vocabulary-only:
  no request for OpenInference or OTel to adopt Assay-Runner.

### Cross-runtime drift experiment package

- Added the `cross-runtime-drift-2026-05` experiment package: workload
  contract, OpenAI Agents and Google GenAI implementations, stdlib contract
  checker, delegated runner workflow, live Arm A0/B0 baselines, and a
  stdlib drift comparator.
- Added path projection v0 and network projection v0 as additive report
  projections. Raw observed values remain the source of truth; declared
  projection aliases add logical labels such as `workdir/input`,
  `workdir/output`, and `dns` without claiming semantic equivalence.
- Added runtime/noise taxonomy v0 as vocabulary-only metadata. The taxonomy
  travels with drift reports but does not yet classify raw paths or endpoints
  heuristically.
- Added drift-report provenance v0 and render metadata so each report
  records the capture anchor, comparator/render anchor, workflow URL, runner
  schema versions, and whether committed reports are re-renders over
  unchanged raw archives.
- Polished drift-report UX: compact projection mappings, per-arm unmatched
  summaries, deduplicated Markdown `raw -> projected` examples, and
  regenerated committed drift reports with self-contained provenance.

### Runner artifact contracts and schema validation

- Hardened Linux cgroup root selection for delegated runner-spike runs under
  `sudo`: systemd `*.scope` cgroups are now treated as leaf scopes and the
  runner ascends to the surrounding slice before creating the Assay session
  cgroup. This avoids `Operation not supported (os error 95)` failures on
  revived self-hosted runner services.
- Added kernel-event metadata support for `openat` / `openat2` observations:
  decoded flags, access mode, operation flags, return value, and
  success/error status where available. This improves file-operation
  granularity without overclaiming read/write semantics outside the captured
  metadata.
- Added JSON Schema sidecars for the runtime drift report and kernel event
  NDJSON line shape, plus stdlib schema-walker tests that validate committed
  fixtures and examples without adding a test-time `jsonschema` dependency.
- Tightened schema documentation around nullable-required fields, git commit
  anchors versus content-addressed `sha256:` digests, `kind` /
  `event_type` consistency, and committed re-render path conventions.

### Release and CI hygiene

- Synced the Runner-vs-OTel and cross-runtime drift roadmaps so completed
  slices on `main` are clearly distinguished from future follow-ups.
- Kept release-truth wording bounded: experiment artifacts and reports are
  committed evidence packages, not product endorsements, Trust Card claims,
  or policy verdicts.
- Updated the release checklist to include the four `assay-runner-*` crates
  in the Trusted Publishing review, matching the public-crate contract that
  `v3.11.3` established.

### Known follow-ups

- Runtime drift `unmatched_summary` has been locked in
  `assay.runner.runtime_drift.v0.2`; historical v0 reports remain
  readable, and new re-renders should use the v0.2 schema.
- Drift projections still avoid heuristic path/runtime noise classification.
  Unknown raw values remain raw until a declared projection rule or a future
  taxonomy rule classifies them.
- The Runner-vs-OTel and cross-runtime experiments do not yet include
  statistically powered overhead measurements or an L3 generic kernel
  observability comparison.

### Non-change

- No new Trust Basis or Trust Card claim family ships in this release.
- No new public guarantee is made for the `assay-runner-*` crates beyond the
  `v3.11.3` framing: they remain internal/experimental substrate crates
  published so `assay-cli` can resolve its default `runner` feature.
- The cross-runtime drift reports are comparison/projection artifacts, not a
  policy verdict about which runtime is "better" or "safer".

## [3.11.3] - 2026-05-23

> **Public crate contract update for Assay-Runner substrate.**
>
> `v3.11.0`, `v3.11.1`, and `v3.11.2` are all partial-publish lines on
> crates.io: `assay-cli` did not publish on any of them. This release
> registers the four `assay-runner-*` crates in the explicit public-crate
> allow-list — a deliberate policy decision, not a manifest hot-fix —
> and is the first complete crates.io publish line since `v3.10.2`.

### Why `v3.11.2`'s manifest flip alone wasn't enough

`v3.11.2` removed `publish = false` from the four runner crates so cargo
could resolve them at publish time, but it did not update
`scripts/ci/check-public-crate-policy.sh`. That script enforces an
explicit allow-list of public crates as a release-truth-line contract
against both `Cargo.toml` metadata and `publish_idempotent.sh`'s `CRATES`
array. Because the script runs inside the release workflow on tag push
(not in PR CI), the divergence between "manifest says publishable" and
"policy allow-list says not allowed" only surfaced after merge, when the
release workflow's policy check blocked the publish chain before any
`cargo publish` ran.

The gate worked as intended. The fix is to acknowledge the policy
decision in the allow-list itself, not to soften the gate.

### Resolution

- `scripts/ci/check-public-crate-policy.sh`: add the four runner crates to
  the `public_crates` array. The comment block now documents the framing:
  these crates are published because `assay-cli` depends on them, with
  explicit internal/experimental wording in their package descriptions;
  adding any new public crate here is a deliberate public-surface
  decision.
- `.github/workflows/ci.yml`: new `Public crate policy` PR-CI job runs
  `check-public-crate-policy.sh` on every PR, so the gate fires before
  tag, not at release time. This is the same defense-in-depth pattern as
  the `Publish-shape guardrail (assay-cli)` job added in `v3.11.1`.
- `docs/contributing/WAVE0-GATES.md`: document the runner crates as
  published-but-not-semver-checked, and note the new PR-CI guardrail.

### What this changes for consumers

- `cargo install assay-cli` works again at `3.11.3` (first complete CLI
  publish since `3.10.2`). Default features include `runner`; CLI ships
  with the hidden internal `runner-spike` command. Users who want a
  runner-free CLI can install with
  `--no-default-features --features tui,sim`.
- `assay-runner-{schema,core,linux,spike}` are visible on crates.io at
  `3.11.3` for the first time. Their package descriptions explicitly
  state: *"Internal/experimental substrate for Assay measured-run
  workflows … No standalone product guarantee; API surface remains narrow
  and intentionally undocumented for third-party use; semver tracks the
  Assay workspace."* They are **not** in the Wave 0 library semver
  allowlist.

### Known issue with the `v3.11.0`, `v3.11.1`, and `v3.11.2` lines

All three earlier `v3.11.x` releases on crates.io are partial-publish:

- `assay-cli` was not published; the published CLI line remains at
  `3.10.2` for those tags.
- `v3.11.0` and `v3.11.1` also did not publish the runner crates.
- `v3.11.2` published the 8 non-runner workspace crates at `3.11.2` but
  blocked at the policy gate before publishing the runner crates or
  `assay-cli`.

The corresponding GitHub Releases stay in place as a record of what
shipped on the binary-tarball side and what the policy gate caught. Use
`v3.11.3` for the first complete crates.io publish line.

### Non-change

- No behavioural change to Assay-core / Trust Basis consumers, NDJSON
  evidence, Trust Basis diff v1, Runner v0 archive contracts, or the
  cross-runtime diff v0 surface.
- The `Publish-shape guardrail (assay-cli)` PR-CI job from `v3.11.1`
  stays in place alongside the new `Public crate policy` job.

## [3.11.2] - 2026-05-23

> **Intended as the corrected publish line; in practice a third partial-publish
> line.** `v3.11.2` flipped the four Assay-Runner crates from `publish = false`
> to publishable with internal/experimental framing, so cargo could resolve
> them at publish time. The release-workflow policy gate
> (`scripts/ci/check-public-crate-policy.sh`) then correctly blocked the
> publish chain because the policy allow-list still listed only the original
> 10 public crates. No runner crate and no `assay-cli` was published from this
> tag; `assay-cli` remained at `3.10.2` on crates.io. See `[3.11.3]` for the
> follow-up policy PR that registers the runner crates in the allow-list and
> mirrors the policy check to PR CI. The framing of the runner crates as
> internal/experimental substrate stays unchanged from what landed in this
> release.

### Why `v3.11.1`'s `optional = true` was not enough

`v3.11.1` attempted to make `assay-cli` publishable by marking its
`assay-runner-{schema,core,linux}` deps as `optional = true` and passing
`--no-default-features --features tui,sim` at publish. That was a wrong
mental model of `cargo publish`. Cargo verifies **every** dep listed in the
crate manifest at publish time, regardless of which features are active:
optional deps must still have a `version` pin **and** that version must be
resolvable from crates.io. Path-only deps without versions are rejected with
`all dependencies must have a version requirement specified`. There is no
combination of feature flags that lets a published crate keep deps on
internal `publish = false` workspace siblings.

### Resolution: flip the four Assay-Runner crates to `publish = true`

The four runner crates ship to crates.io as of `v3.11.2`:

- `assay-runner-schema`
- `assay-runner-core`
- `assay-runner-linux`
- `assay-runner-spike`

Each package description now opens with
*"Internal/experimental substrate for Assay measured-run workflows… No
standalone product guarantee; API surface remains narrow and intentionally
undocumented for third-party use; semver tracks the Assay workspace."*

This is a deliberate reframing, not an extraction. The crates were already
*extraction-ready* per the Phase 2D Slice 6B work that landed for `v3.11.0`;
making them resolvable on crates.io is the smallest step that restores
`cargo install assay-cli` without splitting the repo. Slice 7 (separate
repo extraction) stays closed; the burn-in criteria in
`docs/reference/runner/phase-2d-consolidation-audit.md` continue to apply.

### What this changes for consumers

- `cargo install assay-cli` works again at `3.11.2`, with the `runner`
  feature on by default (binary includes `assay runner-spike` per workspace
  parity).
- `cargo install assay-cli --no-default-features --features tui,sim`
  still works and produces a `runner-spike`-free CLI for consumers who want
  the publishing-minimal surface.
- The four runner crates are visible on crates.io but **not** part of the
  public Assay API contract. Third parties using them do so at their own
  risk; their semver is the workspace's semver.
- `scripts/ci/publish_idempotent.sh` now publishes the four runner crates
  in dependency order (`assay-runner-schema → assay-runner-linux →
  assay-runner-core → assay-runner-spike`) between `assay-monitor` and
  `assay-sim`. The per-crate `--no-default-features --features tui,sim`
  override for `assay-cli` is removed; default features are sufficient now.

### Non-change

- No behavioural change to Assay-core / Trust Basis consumers, NDJSON
  evidence, Trust Basis diff v1, Runner v0 archive contracts, or the
  cross-runtime diff v0 surface.
- The `Publish-shape guardrail (assay-cli)` PR-CI job added in `v3.11.1`
  stays in place as defense-in-depth: it will not fire today (none of
  `assay-cli`'s non-optional workspace deps are `publish = false`; other
  workspace crates such as `assay-ebpf`, `assay-xtask`, and the adapter
  crates remain `publish = false` by design, outside `assay-cli`'s dep
  surface), but will catch a future regression if a new `publish = false`
  workspace crate is added back and reaches `assay-cli`'s non-optional
  dep set.

## [3.11.1] - 2026-05-23

> **Publish-path hot-fix for `assay-cli`.**
>
> No behavioural change for repo / workspace consumers or for GitHub Release
> binary tarballs. This release exists only to make `assay-cli` publishable to
> crates.io again, restoring the `cargo install assay-cli` install path that
> was incomplete in the `v3.11.0` line.

### Known issue with the `v3.11.0` crates.io line

The `v3.11.0` release published 8 of the 9 workspace crates to crates.io
(`assay-common`, `assay-evidence`, `assay-core`, `assay-metrics`, `assay-policy`,
`assay-mcp-server`, `assay-monitor`, `assay-sim`). `assay-cli@3.11.0` failed to
publish because the Slice 6B extraction-readiness work (PR #1325) had
introduced direct dependencies in `assay-cli/Cargo.toml` on the internal
`assay-runner-{schema,core,linux}` crates, all of which are `publish = false`.
Cargo refused the publish with `no matching package named "assay-runner-core"
found`. The release workflow only exercises `cargo publish` on tag push, so
PR CI never saw the failure mode.

`v3.11.0` GitHub Release binaries and the workspace at tag `v3.11.0` are
unchanged and correct; this hot-fix only changes how the CLI is packaged for
crates.io.

### Fix

- `crates/assay-cli/Cargo.toml`: `assay-runner-{schema,core,linux}` deps are
  now `optional = true`, gated behind a new `runner` feature that is in the
  default feature set. Repo builds, `cargo install` from a checkout, and the
  release binary tarballs are byte-equivalent in behaviour to `v3.11.0`.
- The `runner-spike` command (`assay runner-spike`) is now gated behind
  `#[cfg(feature = "runner")]` in `commands/mod.rs`, `args/mod.rs`, and
  `dispatch.rs`. Default builds keep the command; `cargo install assay-cli`
  from crates.io (which deactivates `runner`) ships an `assay-cli` without
  the hidden internal command. This matches the existing CHANGELOG framing
  that `assay runner-spike` is internal-only and outside the public CLI
  contract.
- `scripts/ci/publish_idempotent.sh`: `assay-cli` is published with
  `--no-default-features --features tui,sim`, so the optional runner deps
  are not required to resolve from crates.io. All other workspace crates
  publish unchanged.

### Guardrail

- The runner workflows (`runner-spike-delegated.yml`, `runner-spike-sdk.yml`)
  continue to build `assay-cli` with `--no-default-features`, but now also
  pass `--features runner` so the delegated gates and SDK-correlation gates
  still have the `runner-spike` command available. Default-feature builds
  (release.yml binary tarballs, workspace dev) need no change.
- A `cargo publish --dry-run -p assay-cli --no-default-features --features
  tui,sim` smoke job has been added to PR CI so a future regression of this
  shape is caught before tag, not after.

### Non-change

- No behavioural change to Assay-core / Trust Basis consumers, NDJSON
  evidence, Trust Basis diff v1, Runner v0 archive contracts, or the
  cross-runtime diff v0 surface. Workspace version pin bumps from
  `3.11.0` to `3.11.1` only.

## [3.11.0] - 2026-05-23

> **Internal Assay-Runner measured-run contracts and extraction-ready substrate.**
>
> Assay-Runner remains an **internal** measured-run subsystem of Assay. This release
> is **not** a standalone Runner release; the Runner crates stay `publish = false`
> and Assay still owns measurement semantics. This release exists to mark a
> durable line on `main` for what has accumulated since `v3.10.2`: Runner v0
> archive contracts, a qualified second runtime fixture, the cross-runtime
> diff v0 surface, and the extraction-ready crate split.

This minor release has no breaking change for existing Assay-core / Trust Basis
consumers. NDJSON evidence, Trust Basis diff v1, the three-family receipt
adoption surface, and the existing public `assay` CLI verbs and their outputs
are unchanged versus `v3.10.2`. The only new CLI surface is `assay runner-spike`,
which is `hide = true`, internal-only, and explicitly outside the public CLI
contract; it exists to back the Runner v0 archive contracts and is not part of
the stable interface.

### Assay-Runner v0 measured-run contracts are now durable

The `assay.runner.*.v0` artifact contracts that Phase 1 produced are now living
under explicit ownership rather than under the spike crate. Same wire shape,
new boundary.

- New publish-disabled crate `assay-runner-schema` hosts the v0 data
  structures and constants for
  `assay.runner.observation_health.v0`,
  `assay.runner.capability_surface.v0`,
  `assay.runner.correlation_report.v0`,
  `assay.runner.sdk_event.v0`, and
  `assay.runner.archive_manifest.v0`.
- New publish-disabled crate `assay-runner-core` hosts archive assembly,
  layer normalizers, and the `RunnerSpikeArchive` writer that turns measured
  events into a deterministic `.tar.gz` bundle with `sha256:<hex>` per-file
  digests.
- New publish-disabled crate `assay-runner-linux` hosts cgroup v2 placement
  primitives (`CgroupManager`, `SessionCgroup`) — Linux platform adapter
  surface only.
- `assay-cli` consumes Runner via these three crates directly. The
  `assay-runner-spike` crate is retained as a legacy alias for readers of
  pre-extraction history; no in-workspace consumer depends on it for
  production code. A mechanical absence check in
  `scripts/ci/assay_runner_lane_check.py --self-test` enforces this
  invariant going forward.
- The four structural extraction blockers tracked under Phase 2D are
  resolved on main (Slices 1, 2, 3, 6B). Slice 7 — repository extraction —
  stays closed; the consolidation gate has moved from a passive 4–6 week
  calendar wait to explicit burn-in criteria documented in
  `docs/reference/runner/phase-2d-consolidation-audit.md`.

### Qualified second runtime fixture (Gemini)

`runner-fixtures/gemini-google-genai/` is a second qualified runtime line
producing artifacts under the same v0 contracts as the OpenAI Agents
fixture. The fixture passes idempotent capability-diff acceptance on the
delegated `assay-bpf-runner` host. Identity probe, deterministic local
provider, recorded cassette, and acceptance harness all live in-tree.

The fixture-package boundary now lives at top-level `runner-fixtures/`
(formerly `tests/fixtures/runner-spike/`); Node fixture renamed to drop
the `-js` suffix.

### Cross-runtime diff v0 (frozen under A1+B3+C1)

New artifact contract `assay.runner.cross_runtime_diff.v0` for comparing
the v0 capability surface across two distinct qualified runtimes.

- Normative golden shape at
  `docs/reference/runner/golden/cross-runtime-diff-s5-gemini-v0.json`.
- JSON Schema 2020-12 sidecar for the clean-output shape at
  `docs/reference/runner/schema/cross-runtime-diff-v0-clean.schema.json`.
  This schema is the wire-contract anchor consumers should pin against.
- Decision record at `docs/reference/runner/cross-runtime-diff-decisions.md`
  documents the A1+B3+C1 choices (work-dir prefix canonicalization,
  side-band SDK metadata, out-of-scope binding-id/policy-outcome
  comparison).
- Reference projector at
  `scripts/ci/assay_runner_cross_runtime_diff_validate.py`.
- Explicit `non_claims` carry through: no acceptability judgment, no
  declared-capability input, no derived binding identity, no filename
  semantic equivalence, no SDK capability equivalence across runtimes.

### Consumer side (Harness)

The companion Harness recipe at [`Rul1an/Assay-Harness`](https://github.com/Rul1an/Assay-Harness)
can now consume Runner archives and the cross-runtime diff artifact
separately (`verify-runner`, `runner compare`, `runner cross-runtime
report`, `runner cross-runtime gate`). Assay still owns measurement
semantics; Harness only validates, projects, and gates. The Harness side
is `Rul1an/Assay-Harness@v0.6.0` at the time of this release.

### Documentation

- New Phase 1 + Phase 2 retrospective at
  `docs/notes/ASSAY-RUNNER-PHASE-1-AND-2-RETROSPECTIVE-2026-05-22.md`
  collapses the whole arc into one read.
- New read-only walkthrough at
  `docs/reference/runner/examples/measured-run-proof-bundle.md` shows what
  one measured-run bundle contains.
- New conceptual note at
  `docs/notes/ASSAY-RUNNER-MEASURED-RUNS-2026-05-23.md` explains why
  measured runs are conceptually distinct from traces.
- `docs/reference/runner/extraction-roadmap.md` defines the Phase 2D
  slice sequence and the per-PR boundary discipline rule.
- `docs/reference/runner/phase-2d-consolidation-audit.md` replaces the
  passive 4–6 week wait with burn-in criteria.
- README adds a short "Internal: Assay-Runner" section pointing at the
  reference index, the consolidation audit, and the measured-run
  walkthrough.

### Non-claims (explicit)

- Assay-Runner is **not** released as a standalone product. Runner crates
  stay `publish = false`.
- Slice 7 (repository extraction) is **not** opened. It stays gated on
  consolidation burn-in plus a concrete external consumer use case.
- macOS / Windows measurement paths are **not** in scope. They remain
  separate platform spikes (see `platform-and-extraction-readiness.md`).
- No new public-CLI surface is added on the Assay side; only the internal
  crate boundary moved. `assay-cli` flags and outputs are unchanged for
  existing users.
- The cross-runtime diff carries explicit non-claims (no semantic
  equivalence between runtimes); consumers (Harness or otherwise) must
  not contradict them.

### Release operations

- Workspace version bumped `3.10.2` → `3.11.0`.
- All workspace dependency pins for internal crates updated to `3.11.0`.
- `Cargo.lock` refreshed.
- P57 seeding pack updated to use the `v3.11.0` release-truth line.

## [3.10.2] - 2026-05-17

This patch release carries the same three-family adoption surface as `v3.10.1`
and fixes the release asset preflight so Windows `.sha256` files with CRLF line
endings are accepted when the checksum target and hash are otherwise correct.
It does **not** add runtime behavior, a new claim-visible receipt family,
Harness semantics, or a new external claim.

### Release Operations

- Tolerated CRLF line endings when parsing release checksum target filenames.
- Added a regression test for the Windows `.zip.sha256` shape that blocked the
  `v3.10.1` GitHub Release creation after the build matrix had succeeded.
- Updated the P57 seeding pack to use the `v3.10.2` release-truth line for
  outward proof, theory, mapping, and adoption links.

## [3.10.1] - 2026-05-17

This patch release packages the post-`v3.10.0` three-family adoption surface
under one versioned Assay line. It focuses on release-truth and shareability:
the proof page, longform receipt note, assurance mapping note, and three
search-intent adoption pages now travel together under the same tag. It does
**not** add runtime behavior, a new claim-visible receipt family, Harness
semantics, a compliance claim, a partnership claim, or a hosted surface.

### Docs / Adoption

- Added three compact adoption paths for the released claim-visible receipt
  families:
  - [Evidence Receipts from Promptfoo JSONL](docs/use-cases/evidence-receipts-from-promptfoo-jsonl.md)
    for selected eval outcome receipts.
  - [OpenFeature EvaluationDetails to CI Review Artifact](docs/use-cases/openfeature-evaluationdetails-to-ci-review-artifact.md)
    for bounded runtime decision receipts.
  - [CycloneDX ML-BOM Model to Inventory Receipt](docs/use-cases/cyclonedx-mlbom-model-to-inventory-receipt.md)
    for selected model inventory/provenance-reference receipts.
- Updated README, docs homepage, use-cases index, and MkDocs navigation so the
  three adoption routes appear in the intended order: Promptfoo first,
  OpenFeature second, CycloneDX third.
- Tightened the P57 ecosystem seeding pack around one release-truth line:
  outward links for proof, theory, mapping, and adoption surfaces should use
  this tag or a later release tag rather than `main`.

## [3.10.0] - 2026-05-11

This minor release turns the post-`v3.9.2` audit/refactor sweep into a
versioned line. It focuses on maintainability, workflow security, evidence
boundary tests, and release-operability. It does **not** add a new public
claim-visible Trust Basis family, trust score, hosted service, compliance
claim, or MCP registry publication claim.

### Evidence Portability

- Added the first bounded LiveKit tool-action importer slice for the P47
  acted-family exploration. The importer keeps call/action pairing explicit and
  stays in the same receipt-boundary discipline as the existing external
  surfaces: bounded evidence in, no raw upstream transcript or transport state
  as Assay truth, and no new Trust Basis claim family in this release.
- Tightened external sample boundaries so fixture/documentation examples remain
  clear about what is released, what is importer-only, and what remains
  planning or probe material.

### Refactor / Maintainability

- Completed the Wave 51 hotspot split across the runner, sandbox, MCP proxy,
  and Trust Basis areas while preserving stable public facades. The work moved
  large implementation blocks into focused internal modules and added split
  review artifacts/gates so future changes can be reviewed by boundary instead
  of by monolithic files.
- Added MCP proxy characterization contracts before splitting policy branches,
  and froze Trust Basis behavior before moving generation, classifiers,
  canonical serialization, and tests into a more maintainable layout.
- Removed stale CLI/dead paths and pruned dependency hygiene drift without
  changing supported behavior.

### Security / Assurance

- Hardened local MCP registry credential hygiene: `.mcpregistry_*` token files
  are ignored, nested tracked/unignored token paths are guarded, and security
  docs now call out rotation when local credentials may have leaked.
- Added high-signal OWASP MCP security fixtures for token/log exposure,
  metadata/tool poisoning, and sandbox command-injection boundaries.
- Added opt-in public API and mutation smoke gates for critical pure modules,
  including Trust Basis classifiers/diff logic and sandbox degradation helpers.

### CI / Release

- Reworked self-hosted runner health into a label-specific monitor that reports
  real `assay-bpf-runner` backlog instead of generic GitHub queue pressure.
- Skipped expensive Kernel Matrix artifact/self-hosted work before eBPF diff
  detection when no eBPF files changed.
- Reused built CLI artifacts across action contract tests instead of rebuilding
  the release binary in every consumer job.
- Added a high-confidence `zizmor` workflow-security lane, removed
  high-confidence template-injection patterns, narrowed workflow permissions,
  disabled persisted checkout credentials where not needed, and removed the
  `pull_request_target` Dependabot maintenance path.
- Replaced the third-party release creation action with native `gh release`
  commands and disabled release build caches to remove cache-poisoning ambiguity
  from the publish lane.

### Docs / Distribution

- Led the README with explicit evidence levels (`verified`, `self_reported`,
  `inferred`, `absent`) and a compact "what ships today" table before the
  deeper Trust Compiler lineage.
- Added an MCP Registry discovery audit and tightened MCP Registry publish
  docs around the canonical `io.github.Rul1an/assay-mcp-server` identity,
  release-attached `server.json`, stale legacy registry entry handling, and
  third-party directory freshness checks.
- Documented the GitHub Action PATH compatibility contract and kept
  release-truth wording explicit about what is merged, released, and separately
  published.

## [3.9.2] - 2026-05-04

This patch release prepares the post-canonicalization evidence receipt surface
for versioned sharing. It makes the proof page and assurance mapping note
available under an immutable Assay tag, carries forward the released Assay
`v3.9.1` / Assay Harness `v0.3.2` proof artifacts, and keeps the seeding pack
under release-truth guardrails. It does not add a new public claim-visible
receipt family, Harness family semantics, compliance claim, partnership claim,
or broad launch surface.

### Evidence Portability

- Selected Pydantic Evals as the next evidence-seam hardening candidate via
  `P9b`, but kept the scope deliberately small: one reduced case-result
  artifact derived from `EvaluationReport.cases[]`, possible importer-only
  support only if the live recut succeeds, no raw `ReportCase` contract, no
  full `EvaluationReport` import, no Logfire/trace/span payloads, no Trust
  Basis claim, no Harness recipe, and no public receipt-family story.
- Recut the Pydantic Evals sample around `pydantic-evals==1.89.1` and one
  reduced case-result artifact. The new fixtures carry `case_name`, bounded
  assertion/score results, and export timestamp only; broad `ReportCase`
  fields such as raw input, expected output, model output, trace, and span data
  remain rejected.
- Added P9c as the Pydantic reduced case-result receipt readiness freeze. The
  lane stays pre-importer: `EvaluationReport.cases[]` remains discovery input,
  the reduced case-result artifact is the possible import unit, `ReportCase`
  is not the contract unit, `case_name` is the only docs-backed v1 identity,
  and any importer-only P9d work must first preserve the
  no-trace/no-Logfire/no-output boundary.
- Added P9d importer-only support for bounded Pydantic Evals reduced
  case-result artifacts via `assay evidence import pydantic-case-result`.
  The new `assay.receipt.pydantic.case_result.v1` receipt lane is bundleable,
  schema-visible, and explicitly `trust_basis_claim: null`; it does not add a
  Trust Basis claim, Trust Card row, Harness recipe, raw `ReportCase` import,
  full `EvaluationReport` import, Logfire/trace import, or evaluator/model
  correctness claim.
- Refreshed the Mastra ScoreEvent sample against `@mastra/core` `1.29.1` and
  `@mastra/observability` `1.10.2` after upstream confirmed `ScoreId` had
  shipped. The strong fixture now carries live-backed `score_id_ref`; the v1
  importer keeps the field optional for older reduced artifacts and
  compatibility fixtures.
- Added P14d as the Mastra score-receipt Trust Basis readiness freeze. The
  existing `assay.receipt.mastra.score_event.v1` lane remains importer-only
  with `trust_basis_claim: null`; any future
  `external_score_receipt_boundary_visible` claim must first define exact claim
  semantics, Trust Card impact, and Harness posture.
- Added a Trust Basis CLI regression guard proving
  `external_score_receipt_boundary_visible` remains a planning-only candidate,
  not a registered claim id accepted by `assay trust-basis assert`.

### Docs

- Added
  [Evidence Receipts in Action](docs/notes/EVIDENCE-RECEIPTS-IN-ACTION.md),
  a static proof page with checked-in artifacts generated from the released
  Assay `v3.9.1` binary and Assay Harness `v0.3.2` gate/report surface. The
  page shows the three released receipt families, their exact Trust Basis claim
  IDs, and the raw diff JSON to Markdown/JUnit projection split without adding
  a new product surface or integration claim.
- Added a copyable GitHub Actions proof snippet to the Evidence Receipts in
  Action page. The snippet verifies the checked-in proof bundles with the
  released Assay `v3.9.1` binary, writes a small job summary, and uploads
  canonical/projection artifacts without adding a required workflow or new
  runtime semantics.
- Added the
  [Evidence Receipt Assurance Mapping](docs/notes/EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md)
  note to map the three released receipt families to assurance questions,
  visible evidence boundaries, and explicit non-claims. This is not a
  compliance checklist or legal interpretation.
- Added the P57 ecosystem seeding pack with a one-link repo-native post,
  release-truth link rules, stopping rules, and explicit guards against
  promoting main-only notes as released surfaces.

### CI / Release

- Added a reproducible `mkdocs build --strict` CI job while keeping repo
  crosslinks in the existing link-checker path.
- Hardened the idempotent crates.io publisher so it waits for each newly
  published workspace crate to become visible through the crates.io API before
  publishing the next dependent crate.
- Narrowed self-hosted eBPF CI triggers so release-publish helper changes do
  not leave optional BPF runner jobs queued when the self-hosted runner is
  offline.

## [3.9.1] - 2026-04-29

This patch release publishes the final public three-family evidence receipts
note under an immutable Assay release tag. It does not add runtime behavior,
Trust Basis claims, receipt families, schema semantics, or Harness semantics.

### Release Truth

- **Versioned public note**:
  [Evidence Receipts for AI Outcomes, Runtime Decisions, and Model Inventory](docs/notes/EVIDENCE-RECEIPTS-FOR-AI-OUTCOMES-RUNTIME-DECISIONS-MODEL-INVENTORY.md)
  now points to the released Assay `v3.9.1` surface and Assay Harness `v0.3.2`
  compatibility line, while keeping the same downstream-only boundary:
  Promptfoo assertion component results, OpenFeature boolean `EvaluationDetails`
  outcomes, and CycloneDX `machine-learning-model` components are bounded
  receipt families, not official integrations or upstream truth claims.

## [3.9.0] - 2026-04-29

This minor release turns the post-v3.8.0 consolidation program into a
user-facing release line. It does not add new Trust Basis claims or receipt
families. Instead, it makes the existing trust compiler surface easier to gate,
inspect, review, and bind to the MCP policy/tool surfaces that governed a
decision.

### Trust Compiler

- **Trust Basis assertions**: `assay trust-basis assert` can now gate one
  canonical `trust-basis.json` artifact against generic
  `--require <claim-id>=<level>` predicates. The command is claim-id based,
  emits text or `assay.trust-basis.assert.v1` JSON, exits `0` on pass, exits
  `1` on policy mismatch, and keeps input/config/runtime failures on `2+`.
- **Receipt schema CLI**: `assay evidence schema list/show/validate` exposes
  the v3.8.0 receipt schema registry as a command-line surface. It lists
  receipt payload and importer-input schemas, shows schema metadata before raw
  JSON Schema content, validates JSON or JSONL artifacts, and keeps Mastra
  marked as importer-only rather than a public Trust Basis claim family.
- **Static Trust Card HTML**: `assay trustcard generate` now writes
  `trustcard.html` beside `trustcard.json` and `trustcard.md`. JSON remains the
  canonical Trust Card artifact; Markdown and single-file HTML are deterministic
  reviewer projections with no remote assets, JavaScript requirement, scores,
  badges, or second classifier.
- **Policy snapshot digest visibility**: supported MCP `assay.tool.decision`
  events now project `policy_snapshot_digest`,
  `policy_snapshot_digest_alg`, `policy_snapshot_canonicalization`, and
  `policy_snapshot_schema` from the existing `policy_digest` when available.
  `policy_snapshot_digest` is the self-describing reviewer projection of
  `policy_digest`; the values match on supported paths, and the snapshot field
  cluster is produced atomically. This is a review binding only; it does not
  claim the policy is correct, sufficient, safe, approved, complete,
  retrievable, exportable, or embedded.
- **Tool definition digest visibility**: supported MCP `tools/list` to
  `tools/call` decision paths can now project an atomic `tool_definition_*`
  field cluster onto `assay.tool.decision` events. The digest is computed over
  the bounded observed tool-definition surface using
  `jcs:mcp_tool_definition.v1` and excludes `x-assay-sig`, top-level
  vendor/provider metadata, annotations, display hints, raw registry bodies,
  runtime results, and inferred `tools/call` fields. This is review visibility
  only; it does not claim tool safety, signature validity, signer trust,
  registry truth, or implementation truth.

### Product Truth

- **Product surface alignment**: README, docs home, scope docs, CLI about text,
  AI-context notes, and the P52-P56 consolidation plan now describe Assay as a
  CI-native evidence and trust compiler. The wording separates Assay core from
  Assay Harness, keeps external receipt lanes downstream-only, and avoids
  partnership, integration, correctness, safety, or compliance-truth claims.

## [3.8.0] - 2026-04-29

This minor release turns the v3.7.0 three-family receipt surface into a more
external-ready contract line. The receipt families and Trust Basis claims stay
the same; the new work is machine-readable schema coverage and release-truth
alignment for consumers that need to produce or inspect bounded receipts.

### Receipt Contracts

- **Receipt schema registry**: `docs/reference/receipt-schemas/` now contains
  JSON Schema contracts for the supported Promptfoo, OpenFeature, CycloneDX
  ML-BOM, and Mastra receipt payloads plus their supported importer input
  artifact shapes.
- **Receipt family matrix links schemas**:
  `docs/reference/receipt-family-matrix.json` now points each claim-visible
  family at its receipt and input schemas. Mastra remains documented as
  importer-only: schema-covered, bundleable, and Trust Basis-readable, but not
  part of the three claim-visible public families.
- **Schema validation tests**: importer-generated receipt payloads and supported
  input artifacts are validated against the registry, keeping prose, fixtures,
  and emitted payloads in lockstep.

### Release Truth

- The three-family note is part of the v3.8.0 release line instead of living
  only as post-v3.7.0 main-branch docs.
- Trust Card schema v5 wording is tightened around the 10-claim surface. There
  are no new Trust Basis claims in this release.

## [3.7.0] - 2026-04-29

This minor release makes the first three-family evidence-portability surface
release-ready. Assay can now reduce selected external eval outcomes, runtime
decision details, and model inventory/provenance surfaces into bounded receipts,
compile supported receipt families into Trust Basis, and keep the same
claim-level boundary discipline as the earlier Promptfoo lane.

### Trust Compiler

- **Three receipt families are claim-visible**: supported eval, decision, and
  inventory receipt bundles can now surface bounded Trust Basis boundary claims:
  `external_eval_receipt_boundary_visible`,
  `external_decision_receipt_boundary_visible`, and
  `external_inventory_receipt_boundary_visible`. These claims mean the supported
  receipt boundary and provenance are visible; they do not mean upstream eval
  correctness, flag-decision correctness, model safety, dataset approval, BOM
  completeness, license posture, vulnerability posture, or compliance truth.
- **OpenFeature decision receipts**: `assay evidence import openfeature-details`
  imports bounded boolean OpenFeature `EvaluationDetails` rows into verifiable
  decision receipt bundles. The v1 lane keeps provider config, evaluation
  context, targeting keys, rules, user identifiers, flag metadata, provider
  metadata, `error_message`, and non-boolean values out of the canonical
  receipt path.
- **CycloneDX ML-BOM model-component receipts**:
  `assay evidence import cyclonedx-mlbom-model` imports one selected
  `machine-learning-model` component as a bounded inventory receipt. The v1
  lane keeps full BOM graphs, model-card bodies, dataset bodies, pedigree,
  vulnerabilities, licenses, metrics, safety posture, and compliance semantics
  out of the receipt.
- **Mastra ScoreEvent receipts**: `assay evidence import mastra-score-event`
  imports reduced, reviewer-safe Mastra ScoreEvent JSONL artifacts into score
  receipt bundles. This lane does not yet add a Trust Basis score claim; it is
  intentionally separate from the three-family public claim surface.
- **Trust Card schema v5**: Trust Card output now reflects the expanded
  claim table. Consumers must continue to key by stable `claim.id`, not row
  position or row count.
- **Receipt family matrix**: `docs/reference/receipt-family-matrix.json` records
  each supported receipt family, event type, Trust Basis claim, included fields,
  excluded fields, and explicit non-claims.

### Examples and Docs

- Added OpenFeature, CycloneDX ML-BOM, and Mastra ScoreEvent evidence examples
  plus CLI reference docs for the new importers.
- Updated the evidence contract registry with the new experimental receipt event
  types.

### Notes for Upgraders

- This is a release of bounded receipt compiler lanes, not official integration
  or partnership support for Promptfoo, OpenFeature, CycloneDX, or Mastra.
- Trust Basis and Trust Card consumers should treat the new claim rows as
  additive. Select claims by `claim.id` and tolerate unknown future claims.
- Assay Harness `v0.3.1` is the intended companion release for running the
  Promptfoo, OpenFeature, and CycloneDX recipes over this claim surface.

## [3.6.0] - 2026-04-27

This minor release makes the first external-eval evidence portability lane
release-ready. Assay can now import selected external evaluation outcomes as
bounded evidence receipts, carry them through Trust Basis, and compare claim
artifacts without importing full eval-run truth or claiming model correctness.

### Trust Compiler

- **External eval outcomes as bounded receipts**: Assay now has the first
  evidence-portability lane for selected external eval outcomes. The lane starts
  with Promptfoo assertion-component results, compiles them into Assay evidence
  receipts, carries them through Trust Basis / diff, and keeps the boundary
  explicit: no full eval-run import, no Promptfoo integration claim, and no
  model-correctness truth. See
  [From Promptfoo JSONL to Evidence Receipts](docs/notes/FROM-PROMPTFOO-JSONL-TO-EVIDENCE-RECEIPTS.md).
- **Promptfoo JSONL receipt import**: `assay evidence import promptfoo-jsonl`
  imports strict Promptfoo CLI JSONL rows from
  `gradingResult.componentResults[]` and writes verifiable Assay evidence
  bundles. The v1 lane is deterministic-assertion-first (`equals`, binary
  `0`/`1` component scores) and excludes raw prompt, output, expected value,
  vars, provider payloads, token/cost data, and full JSONL rows.
- **Trust Basis visibility for external receipts**: supported external eval
  receipt bundles can now surface the bounded
  `external_eval_receipt_boundary_visible` claim. The claim means the receipt
  boundary and provenance are visible; it does not mean the upstream eval run
  passed or that Assay imports upstream payloads as truth.
- **Trust Basis diff contract**: `assay trust-basis diff` compares canonical
  Trust Basis artifacts by stable claim identity, reports added / removed /
  improved / regressed / metadata-only changes, and can fail CI only on
  claim-presence or claim-level regressions.

### Examples and Notes

- **Promptfoo evidence sample and recipe path**: the Promptfoo assertion
  grading-result sample is restored on `main`, and the Assay-side note explains
  the evidence portability boundary without positioning this as a Promptfoo
  integration or partnership.
- **Additional bounded evidence examples**: OpenFeature `EvaluationDetails` and
  Guardrails validation-outcome lanes document adjacent evidence units while
  staying clear of provider-config truth, corrected-output truth, and full run
  history.

### Notes for Upgraders

- Trust Basis and Trust Card consumers should keep selecting claims by stable
  `claim.id`, not row position or row count. The external-eval receipt claim is
  additive.
- The Promptfoo lane is downstream evidence portability over existing
  JSONL/assertion surfaces. It is not official Promptfoo support, not a
  partnership claim, and not a full Promptfoo export importer.

## [3.5.1] - 2026-04-06

This patch release keeps the `v3.5.0` trust-compiler surface intact, but makes the
new MCP Registry publication path honest and publishable. It is the first Assay
release line that can ship a real `assay-mcp-server-<version>-linux.mcpb` asset
plus generated official-registry metadata from the same release asset set.

### Release Tooling

- **Official MCP Registry publication foundation**: Release builds now package
  Linux `assay-mcp-server` archives into a real
  `assay-mcp-server-<version>-linux.mcpb` bundle and generate `server.json`
  from the released MCPB asset URL plus SHA-256. This replaces the old
  hand-maintained metadata story with a bounded, supported `mcpb` publication
  path for the official MCP Registry.

### Examples

- **CrewAI event evidence sample**: Assay now ships a small sample-first
  `examples/crewai-event-evidence/` flow that exports bounded CrewAI runtime
  events to NDJSON and maps them into Assay-shaped placeholder evidence without
  promoting CrewAI runtime semantics into Assay truth.

## [3.5.0] - 2026-03-30

This release makes the first bounded MCP authorization-discovery seam public. `K2-A` Phase 1 now
ships in the public Assay line as visibility-only evidence for typed MCP auth-discovery surfaces,
without broadening into an auth-discovery pack, auth-success claims, or compliance theater.

### Trust Compiler

- **`K2-A` Phase 1**: Assay now publicly ships the first bounded MCP authorization-discovery seam on imported MCP traces via `episode_start.meta.mcp.authorization_discovery`. The slice is visibility-only, promotes positively only from typed runtime-observed `WWW-Authenticate` discovery on supported `401` transport paths, and explicitly does **not** imply auth success, scope adequacy, issuer trust, or compliance.

## [3.4.0] - 2026-03-28

This patch release makes the post-`v3.3.0` trust-compiler line public: **`G4-A` Phase 1** (`payload.discovery`), built-in **`P2c`** (`a2a-discovery-card-followup`), and **`K1-A` Phase 1** (`payload.handoff`) now ship in the released binaries and Python wheels. It also refreshes outward-facing package/release communication so the published line matches the actual shipped surface.

### Trust Compiler

- **`G4-A` Phase 1**: The A2A adapter now publicly ships the bounded top-level **`payload.discovery`** seam for discovery / Agent Card visibility on canonical adapter evidence. This remains adapter-emitted, visibility-only evidence with explicit non-goals around validity, trust, or verification semantics. See [PLAN-G4](docs/architecture/PLAN-G4-A2A-DISCOVERY-CARD-EVIDENCE-2026q2.md) and [G4-A freeze](docs/architecture/G4-A-PHASE1-FREEZE.md).
- **`P2c` A2A discovery/card follow-up pack (`a2a-discovery-card-followup`)**: Built-in **A2A-DC-001** / **A2A-DC-002** now ship publicly. The pack mirrors `packs/open/a2a-discovery-card-followup/`, uses `json_path_exists.value_equals` for boolean `true`, and keeps the G4-A / P2c floor semantics (`requires.assay_min_version: ">=3.3.0"`) without a new engine bump. See [MIGRATION — P2c pack](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md#a2a-discovery-card-followup-built-in-pack-p2c) and [PLAN-P2c](docs/architecture/PLAN-P2c-A2A-DISCOVERY-CARD-FOLLOWUP-PACK.md).
- **`K1-A` Phase 1**: `assay-adapter-a2a` now publicly emits a bounded top-level **`payload.handoff`** object on canonical A2A adapter evidence. The seam is always present, promotes positively only for typed `assay.adapter.a2a.task.requested` packets with `task.kind == "delegation"`, and explicitly does **not** promote from `task.updated`, `artifact.shared`, generic-message fallback, or synthetic `unknown-task`. No new pack, engine bump, Trust Basis change, or Trust Card change ships in this slice. See [PLAN-K1](docs/architecture/PLAN-K1-A2A-HANDOFF-DELEGATION-ROUTE-EVIDENCE-2026q2.md) and [K1-A freeze](docs/architecture/K1-A-PHASE1-FREEZE.md).

### Python SDK

- **`assay-it` outward-facing metadata**: The Python package now ships with a package-level README and bounded public metadata that matches the actual surface: `AssayClient`, `Coverage`, `Explainer`, and the pytest fixture. The published package description no longer implies the full Assay CLI or broader trust-compiler surfaces.

### Release Tooling

- **Release notes template truth sync**: GitHub release notes now use the canonical install URL `https://getassay.dev/install.sh` and the canonical action slug `Rul1an/assay-action@v2`, avoiding stale release-copy drift on future tags.

## [3.3.0] - 2026-03-24

This release completes the **first trust-compiler product line** on a single public baseline: canonical Trust Basis, Trust Card schema **2** with **seven** claims (key by stable `claim.id`), G3 authorization-context evidence, pack engine **1.2**, built-in **`mcp-signal-followup`** and **`a2a-signal-followup`**, migration SSOT, and kernel/pack alignment tests. See [MIGRATION-TRUST-COMPILER-3.2.md](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md), [PLAN-P2a](docs/architecture/PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md), [PLAN-P2b](docs/architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md), and [RELEASE-PLAN-TRUST-COMPILER-3.3.md](docs/architecture/RELEASE-PLAN-TRUST-COMPILER-3.3.md). Pack `requires.assay_min_version: ">=3.2.3"` remains the **evidence-substrate floor**; **v3.3.0** is the first release embedding both built-in companion packs in release binaries.

### Trust Compiler

- **P2b A2A companion pack (`a2a-signal-followup`)**: Built-in pack with three **presence-only** rules on canonical adapter evidence — **A2A-001** (`assay.adapter.a2a.agent.capabilities`), **A2A-002** (`assay.adapter.a2a.task.*`), **A2A-003** (`assay.adapter.a2a.artifact.shared`). Uses existing pack checks (`event_type_exists`); no new engine version. Open mirror under `packs/open/a2a-signal-followup/`. Pack YAML sets `requires.assay_min_version: ">=3.2.3"` (evidence-substrate floor per [MIGRATION-TRUST-COMPILER-3.2.md](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md), same discipline as [PLAN-P2a](docs/architecture/PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md)). **v3.3.0** is the first Assay release with this pack built in. See [PLAN-P2b](docs/architecture/PLAN-P2b-A2A-SIGNAL-FOLLOWUP-CLAIM-PACK.md).
- **H1 — Trust kernel alignment & release hardening**: Single migration SSOT ([MIGRATION-TRUST-COMPILER-3.2.md](docs/architecture/MIGRATION-TRUST-COMPILER-3.2.md)), [PLAN-H1](docs/architecture/PLAN-H1-TRUST-KERNEL-ALIGNMENT-RELEASE-HARDENING.md), integration tests for Trust Basis ↔ MCP-001 lockstep and Trust Basis ↔ Trust Card invariants (no new semantics).
- **P2a MCP companion pack (`mcp-signal-followup`)**: Built-in pack with three rules — **MCP-001** uses pack check `g3_authorization_context_present` (engine **v1.2**), sharing the same predicate as Trust Basis `authorization_context_visible` (verified); **MCP-002** / **MCP-003** cover delegation (`delegated_from`) and containment degradation (`assay.sandbox.degraded`). Open mirror under `packs/open/mcp-signal-followup/`. `assay_min_version: >=3.2.3` tracks the prerequisite line (G3 + Trust Card schema 2; **v3.2.3** is the reference tag for that substrate, not for built-in pack presence). **v3.3.0** is the first Assay release with this pack built in — see [PLAN-P2a](docs/architecture/PLAN-P2a-MCP-SIGNAL-FOLLOWUP-CLAIM-PACK.md).
- **Pack engine v1.2**: Adds `g3_authorization_context_present`; bumps `ENGINE_VERSION` in `assay-evidence` (mandate-baseline rules that declared `engine_min_version: "1.2"` now execute with this engine).
- **T1a Trust Basis Compiler MVP**: Assay now ships a canonical `trust-basis.json` compiler surface on `main`, derived from verified bundles with fixed claim keys, fixed evidence vocabularies, and deterministic regeneration.
- **Low-level trust compiler CLI**: Repository builds now expose `assay trust-basis generate <bundle>` for advanced CI, diffing, and review workflows.
- **G3 Authorization Context Evidence**: Supported MCP tool-call paths can merge policy-projected `auth_scheme`, `auth_issuer`, and `principal` onto `assay.tool.decision` evidence; normalization allowlists schemes, trims issuer, rejects JWS-compact and `Bearer ` credential material, and omits whitespace-only principals.
- **Trust Card schema v2**: Trust Basis emits **seven** claims (adds `authorization_context_visible` between delegation and containment); `trustcard.json` uses `schema_version` **2**. Downstream consumers should select claims by stable `id`, not assume a fixed row count.

### Notes

- **Claim-first boundary**: `T1a` ships claim classification in the compiler layer, not in a Trust Card renderer.
- **Deliberate non-goals**: This wave does not yet ship `trustcard.json`, `trustcard.md`, a trust score, a `safe/unsafe` badge, or new signal/pack/engine semantics.

### MCP Security

- **New MCP integrity metrics**: Added `tool_description_integrity`, `tool_output_valid`, and `tool_collision_detect` to cover tool-definition drift, output-schema contracts, and cross-server tool shadowing.

### Observability

- **Runtime monitor output**: `assay monitor` blocked-file events now print structured `dev`, `ino`, `cgroup`, and `rule_id` fields instead of raw payload text.
- **Ring buffer pressure summary**: `assay monitor` now reports emitted and dropped ring-buffer counters for tracepoint, LSM, and socket monitor paths at the end of a run.
- **Metric evaluation spans**: The runner now emits one `assay.eval.metric` span per metric evaluation with stable fields for latency, cached status, pass/fail, unstable state, and error reporting.

### Supply Chain

- **CycloneDX release asset**: Release builds now publish `assay-${VERSION}-sbom-cyclonedx.tar.gz` and `assay-${VERSION}-sbom-cyclonedx.tar.gz.sha256` alongside the existing binaries.

---

## [v3.2.2] - 2026-03-17

### Fixes

- **crates.io publish**: Exclude assay-adapter-api from publish list (Trusted Publishing not configured). Use 3.1.0 from crates.io.
- **crates.io publish**: Broaden grep pattern for token-not-valid skip.

---

## [v3.2.1] - 2026-03-17

### Fixes

- **Windows build**: Gate `std::os::unix::fs::PermissionsExt` with `#[cfg(unix)]` so the Windows release build succeeds.

---

## [v3.2.0] - 2026-03-17

### Release

- **Cross-platform builds re-enabled**: macOS x86_64, macOS aarch64 (Apple Silicon), and Windows x86_64 are back in the release matrix.
- **Runner updates (March 2026)**: `macos-15` (was macos-14), `windows-2025` (explicit version).
- **Install script**: `curl -fsSL https://getassay.dev/install.sh | sh` now supports macOS ARM.

---

## [v3.1.0] - 2026-03-15

### MCP Policy Enforcement (Wave24–Wave42)

- **Typed decisions + Decision Event v2**: Deterministic typed decision outcomes with structured `DecisionData` payloads replacing stringly-typed fields.
- **Obligation execution**: Runtime execution of `log`, `alert`, `approval_required`, `restrict_scope`, and `redact_args` obligations with deterministic evidence emission.
- **Approval enforcement**: `approval_required` blocks tool calls without valid approval artifacts; approval shape is additive evidence.
- **Restrict scope enforcement**: `restrict_scope` narrows tool-call arguments at runtime with evidence of what was restricted and why.
- **Redact args enforcement**: `redact_args` strips sensitive fields from tool-call arguments before forwarding, with redaction evidence markers.
- **Fulfillment normalization**: Obligation fulfillment outcomes are normalized into a stable contract for downstream consumers.
- **Deny/fail-closed evidence convergence**: Deny paths and fail-closed decisions emit consistent, typed evidence with deterministic precedence.
- **Replay diff basis**: Deterministic replay diff buckets with legacy fallback classification for backward compatibility.
- **Evidence compatibility normalization**: Replay evidence compatibility markers for additive reader contracts.
- **Consumer hardening**: Frozen consumer read precedence for `DecisionEvent`, `DecisionData`, and `ReplayDiffBasis` payloads.
- **Context envelope hardening**: Completeness markers and additive metadata on context-envelope payloads.

### BYOS Evidence Store (ADR-015 Phase 1)

- **`assay evidence store-status`**: New diagnostic command — checks connectivity, credentials, inventory, and write access. Supports JSON, table, and plain output. Exit codes: 0 (OK), 1 (connectivity/access failure), 2 (config error).
- **`.assay/store.yaml` config**: Structured YAML configuration for evidence store connection. Precedence: `--store` > `ASSAY_STORE_URL` > config file. Credentials stay in environment variables.
- **Config fallback for push/pull/list**: `--store` is now optional — falls back to `ASSAY_STORE_URL` or `.assay/store.yaml` automatically.
- **Provider quickstart docs**: AWS S3, Backblaze B2, MinIO setup guides.

### Architecture & Documentation

- Architecture-as-code workspace: Structurizr/C4, building blocks, quality scenarios, Obsidian view layer, catalog metadata.
- ADR-027 through ADR-031 closed as implemented contracts.
- Repo-wide architecture gap analysis and roadmap truth sync.
- Release/changelog hygiene: consolidated to single curated CHANGELOG.md.

### Fixes

- Evidence command dispatch is now async (fixes nested tokio runtime panic for BYOS commands).
- `StoreConfig::discover()` returns errors on malformed config files instead of silently ignoring them.

---

## [v3.0.0] - 2026-03-05

### Breaking API Changes

- `assay_core::mcp::policy::ToolPolicy` adds `allow_classes` and `deny_classes`.
- `assay_core::mcp::decision::DecisionData` adds `tool_classes`, `matched_tool_classes`, `match_basis`, and `matched_rule`.
- External struct-literal construction against these types now requires populating the new fields.

### DX and Runtime

- **Coverage v1.1 polish:** `assay coverage` supports `--out-md` for reviewer-friendly markdown output and `--routes-top` for route summary control while JSON remains canonical (`coverage_report_v1`).
- **MCP coverage/session exports:** `assay mcp wrap` supports `--coverage-out` and `--state-window-out` informational artifacts with stable schemas and explicit write logging.
- **Tool taxonomy governance:** MCP policy evaluation and decision metadata include tool taxonomy class matching (`tool_classes`, `matched_tool_classes`) for broader sink/source governance coverage.

### Governance Contracts and Runbooks

- Added/finalized ADR contract line for taxonomy, coverage, session/state window, and coverage DX polish (ADR-027/028/029/030/031).
- Added operational runbooks for taxonomy+coverage and session/state export usage in enterprise workflows.

---

## [v2.12.0] - 2026-01-29

### 🔐 Pack Registry: Enterprise-Grade Supply Chain Security

This release introduces the **Pack Registry Client** (`assay-registry` crate) - a complete implementation of SPEC-Pack-Registry-v1.0.3 for secure remote pack distribution.

### ✨ Major Features

-   **Pack Registry Client** (`crates/assay-registry/`):
    -   HTTP client with token + OIDC authentication
    -   Pack resolution: local → bundled → registry → BYOS
    -   Local caching with TOCTOU protection (integrity verified on every read)
    -   Lockfile v2 for reproducible builds (`assay.packs.lock`)

-   **JCS Canonicalization (RFC 8785)**:
    -   Deterministic JSON serialization for pack digests
    -   Uses `serde_jcs::to_vec()` (bytes, not string) to eliminate encoding issues
    -   Canonical digest format: `sha256:{hex}`

-   **Strict YAML Validation (SPEC §6.1)**:
    -   Pre-scan rejects anchors (`&`), aliases (`*`), tags (`!!`), multi-document (`---`)
    -   Duplicate key detection with correct list-item scoping
    -   DoS limits: max depth 50, keys 10k, string 1MB, input 10MB
    -   Integer range checks: ±2^53 (IEEE 754 safe integer)

-   **DSSE Signature Verification**:
    -   Ed25519 + PAE encoding per DSSE spec
    -   Sidecar endpoint (`GET /packs/{name}/{version}.sig`) for large signatures
    -   Client always prefers sidecar over `X-Pack-Signature` header

-   **Trust Model (No-TOFU)**:
    -   Pinned root keys compiled into binary
    -   Key rotation via signed manifest
    -   Pinned roots survive remote revocation attempts
    -   Runtime expiry checks for manifest keys

### 🧪 GitHub Action v2.1 Test Coverage

-   Contract tests for all v2.1 features:
    -   Pack lint with `eu-ai-act-baseline` + SARIF validation
    -   Fork PR SARIF skip logic
    -   OIDC provider auto-detection (AWS/GCP/Azure patterns)
    -   Attestation gating (push-only, default branch, verified)
    -   Coverage calculation formula

### 🐛 Security Fixes (P0)

-   **Duplicate Key Detection**: Pre-scan catches block mapping duplicates; serde_yaml catches flow mapping duplicates
-   **DSSE Verification**: Signature verification uses canonical JCS bytes (not raw YAML)
-   **List-Item Scoping**: Each list item gets its own scope (fixes false positives for `- a: 1\n- a: 2`)

### 📦 New Crate Published

-   `assay-registry` v2.11.0 on [crates.io](https://crates.io/crates/assay-registry)

### 📚 Documentation

-   `docs/architecture/SPEC-Pack-Registry-v1.md` updated to v1.0.3
-   `docs/architecture/ADR-018-GitHub-Action-v2.1.md` - Action v2.1 design
-   `docs/architecture/SPEC-GitHub-Action-v2.1.md` - Action v2.1 specification
-   Security review documentation in `crates/assay-registry/docs/`

### Test Coverage

-   185 tests in `assay-registry` crate
-   Golden vectors for JCS digest verification
-   DSSE real signature verification tests
-   Trust rotation and revocation tests
-   Cache tamper detection tests
-   Protocol edge cases (304/410/429)

---

## [v2.10.0] - 2026-01-28

### 🎯 Pack Engine: Compliance Rule Packs

This release introduces the **Pack Engine** - a YAML-driven compliance/security/quality rule system for evidence bundle linting, with the first built-in pack for EU AI Act Article 12.

### ✨ Major Features

-   **Pack Engine** (`crates/assay-evidence/src/lint/packs/`):
    -   YAML-defined rule packs with typed checks
    -   Check types: `event_count`, `event_pairs`, `event_field_present`, `event_type_exists`, `manifest_field`
    -   JSON Pointer (RFC 6901) for field addressing
    -   JCS canonicalization (RFC 8785) for deterministic pack digests
    -   Collision policy: compliance packs hard-fail, security/quality last-wins

-   **EU AI Act Baseline Pack** (`packs/eu-ai-act-baseline.yaml`):
    -   `EU12-001`: Event recording (Article 12(1))
    -   `EU12-002`: Operation monitoring - started/finished pairs (Article 12(2)(c))
    -   `EU12-003`: Post-market monitoring - correlation IDs (Article 12(2)(b))
    -   `EU12-004`: Risk identification - policy/denial fields (Article 12(2)(a))

-   **CLI Integration**:
    -   `--pack`: Comma-separated pack references (built-in or file path)
    -   `--max-results`: Limit findings for GitHub SARIF size limits (default: 500)

-   **GitHub Code Scanning Compatible SARIF**:
    -   `locations[]` on all results (including global findings)
    -   `primaryLocationLineHash` for GitHub deduplication
    -   Pack metadata in `tool.driver.properties.assayPacks[]`
    -   `run.properties.disclaimer` for compliance packs
    -   Truncation policy with `run.properties.truncated/truncatedCount`

### 📚 Documentation

-   `docs/architecture/SPEC-Pack-Engine-v1.md` - Complete implementation spec
-   `docs/architecture/ADR-013-EU-AI-Act-Pack.md` - EU AI Act pack design
-   `docs/architecture/ADR-016-Pack-Taxonomy.md` - Pack taxonomy and open core model

### Usage

```bash
# Run EU AI Act baseline checks
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline

# SARIF output for GitHub Code Scanning
assay evidence lint bundle.tar.gz --pack eu-ai-act-baseline --format sarif

# Custom pack file
assay evidence lint bundle.tar.gz --pack ./my-pack.yaml
```

## [v2.4.0] - 2026-01-26

### 🛡️ Phase 5: SOTA Sandbox Hardening

This release delivers **State-of-the-Art** sandbox hardening, addressing MCP security guidance for credential isolation, honest capability reporting, and fork-safe enforcement.

### ✨ Major Features

-   **Environment Scrubbing** (`env_filter.rs`):
    -   Default-deny for secrets (`*_TOKEN`, `*_KEY`, `*_SECRET`, `AWS_*`, `GITHUB_*`)
    -   CLI flags: `--env-allow=VAR=value`, `--env-passthrough=VAR`
    -   Always sets `TMPDIR` to scoped sandbox directory
-   **Landlock Deny-wins Correctness** (`landlock_check.rs`):
    -   Detects "deny inside allow" conflicts that Landlock cannot enforce
    -   Automatic degradation to Audit mode with explicit warning
    -   Prevents false sense of security from unenforceable policies
-   **Fork-Safe pre_exec**:
    -   Eliminated heap allocations in `pre_exec` closure
    -   Uses `std::io::Error::from_raw_os_error()` instead of `anyhow::bail!()`
    -   Syscall-only in critical fork-exec window
-   **Scoped /tmp Isolation**:
    -   UID-based (not `$USER` env which can be spoofed)
    -   Per-run isolation via PID in path
    -   0700 permissions (owner-only)
    -   Prefers `XDG_RUNTIME_DIR` when available
-   **Doctor Deep Dive v2**:
    -   Reports Phase 5 hardening feature status
    -   Reads actual Landlock ABI version from sysfs
    -   Net enforcement correctly reports ABI >= 4 requirement

### 🛠️ CI Improvements

-   **`scripts/ci/phase5-check.sh`**: New quality gate script
    -   `CARGO_TARGET_DIR=/tmp/assay-target` for VM mount compatibility
    -   `--locked` on all cargo commands
    -   Strict Clippy `-D warnings`

### 🐛 Fixes

-   Fixed `unused_assignments` warning on macOS via `#[cfg(target_os = "linux")]`
-   Fixed `io_other_error` Clippy lint (Rust 1.93)
-   Added `#[allow(dead_code)]` for non-Linux Landlock stubs

## [v2.2.0] - 2026-01-23

### 🛡️ SOTA Hardening (Jan 2026)

This release delivers "State-of-the-Art" infrastructure hardening, specifically targeting ARM/Self-Hosted stability and CI reliability. It eliminates supply chain risks and ensures deterministic builds across all platforms.

### ✨ Major Features
-   **Robust ARM Infrastructure**: Implemented a "GoFoss -> Ubuntu Ports" failover loop for all ARM runners. This eliminates flaky `404` errors caused by the unstable `ports.ubuntu.com` mirror.
    -   **Generic Logic**: The failover script aggressively rewrites *any* `ubuntu-ports` source, scrubbing legacy/broken mirrors (e.g. `edge.kernel.org`) from self-hosted runners.
    -   **Optimization**: Automatically skips logic on AMD64 runners (`ubuntu-latest`) to preserve "Fast Path" performance.
-   **Intelligent Gating**:
    -   **Fork Safety**: Self-hosted runners are now strictly gated (`if: fork == false`) to prevent malicious code execution from PR forks.
    -   **Split Smoke**: `ebpf-smoke` is split into `-ubuntu` (for signal) and `-self-hosted` (for depth), ensuring forks still get CI feedback.
-   **Performance "Fast Path"**:
    -   **Install-First**: All apt jobs now attempt `install` before `update`, leveraging fresh runner caches for significant speedups.
    -   **Hardened Flags**: Ubiquitous use of `DEBIAN_FRONTEND=noninteractive` and `--no-install-recommends`.

### 🐛 Fixes
-   **Artifact Sequencing**: Fixed a race condition in `kernel-matrix.yml` (`matrix-test`) where install scripts ran before artifact download.
-   **Supply Chain**: Enforced `--locked` / pinned versions for all `bpf-linker` installations.
-   **Cleanup**: Removed legacy `actions/cache` usage for apt-lists (native disk caching is superior on self-hosted).

## [v2.1.1] - 2026-01-15

### 🛡️ LSM Hardening & Safety

Critical release hardening the BPF-LSM implementation for production readiness.

-   **Verifier Fix**: Resolved BPF verifier rejection (exit code 40) by optimizing `emit_event` (removed zeroing loop).
-   **RingBuf Safety**: Implemented secure, full-buffer copy to prevent uninitialized memory leakage to userspace.
-   **Explicit Deny**: Validated E2E `action: "deny"` enforcement (EPERM blocking).
-   **CI Gate**: Hardened `verify_lsm_docker.sh` to enforce hard failures on blocking misses.

## [v2.0.0] - 2026-01-12

### 🛡️ SOTA Hardening (Phase 5)

This major release delivers the **State-of-the-Art (SOTA)** architecture for robust runtime security, transitioning from "Best Effort" to "Forensically Sound" monitoring.

### ✨ Major Features
-   **Cgroup-First Architecture**: `assay-monitor` and `assay-ebpf` now prioritize cgroup membership over PID tracking, using `bpf_get_current_ancestor_cgroup_id` to prevent nested cgroup escapes. This ensures 100% coverage of short-lived processes.
-   **Forensic Incident Bundles**:
    -   **Secure Atomic Writes**: Implementation of `IncidentBuilder` using `openat`, `O_NOFOLLOW`, `O_EXCL`, and `renameat` to prevent TOCTOU vulnerabilities.
    -   **Unique Identity**: Incident files now use UUID v4 suffixes to guarantee uniqueness.
    -   **Detailed Metadata**: Includes kernel version, session UUID, and process tree context.
-   **eBPF Hardening**:
    -   **Dynamic Offsets**: Removed all hardcoded kernel offsets in favor of runtime resolution via `/sys/kernel/tracing/events/.../format`.
    -   **Extended Coverage**: Added `sys_enter_openat2` probe for modern kernels (Linux 5.6+).
    -   **Safety**: Uses `read_user_str_bytes` with explicit bounds checking safe slices.

### 🐛 Fixes & Polish
-   **CI Reliability**: Complete overhaul of CI pipelines using `sccache` (local backend), `mold` linker (Linux), and single-pass testing. Zero 400 errors from GH Actions Cache.
-   **Windows Support**: Fixed compilation issues in `assay-cli` by guarding Unix-specific cgroup logic.
-   **Golden Tests**: Resolved output mismatches for strict reproducibility.

## [v1.8.0] - 2026-01-11

### 🚀 Runtime Features (System 2 Security)

This release transforms Assay from a static analyzer into a complete **Runtime Security Platform**. It introduces the "System 2" capabilities: detecting and stopping dangerous behavior as it happens.

### ✨ Major Features
-   **Runtime Monitor (`assay monitor`)** *(Linux Only)*:
    -   Uses **eBPF** (extended Berkeley Packet Filter) to trace process behavior safely in kernel space.
    -   Detects file access (`openat`) and network connections (`connect`) in real-time.
    -   **Zero-Overhead**: Highly optimized "Read-First" ring buffer implementation.
-   **Discovery (`assay discover`)**:
    -   Automatically inventory running MCP servers and local configurations.
    -   Detects unmanaged servers and security gaps.
-   **Kill Switch (`assay kill`)**:
    -   Emergency termination of rogue agent processes.
    -   Supports graceful shutdown (SIGTERM) and immediate kill (SIGKILL).

### 🛡️ Hardening
-   **Native eBPF Builds**: CI now builds eBPF artifacts natively (no Docker required), ensuring determinism and stability.
-   **Host Build Protection**: The `assay-ebpf` crate is feature-gated to prevent accidental linking on non-Linux hosts.
-   **Strict Dependencies**: All upstream dependencies are strictly pinned for reproducibility.

### 📚 Documentation
-   **Unified Reference**: Consolidated runtime documentation into `docs/runtime-monitor.md`.
-   **Handoff**: Comprehensive architecture & maintenance guide available for contributors.

## [v1.7.0] - 2026-01-09

### 🛡️ Strict Deprecation Mode
- **Refined Deprecations**: Formal deprecation of v1.x constraints syntax.
- **Strict Mode**: New `--deny-deprecations` flag (and `ASSAY_STRICT_DEPRECATIONS=1` env var) to enforce strict compliance in CI.
- **Migration Guide**: New detailed [v1-to-v2 Migration Guide](docs/migration/v1-to-v2.md).
- **Startup Warnings**: Server/Proxy now emit clear warnings when loading legacy policies.

### Added
- **CLI**: `assay policy validate --deny-deprecations` (and for `run`/`wrap` modes).
- **Docs**: Comprehensive `docs/migration/v1-to-v2.md`.

## [v1.6.0] - 2026-01-09

### Added
- **Policy v2.0 (JSON Schema)**: Official support for JSON Schema constraints (`schemas:`) replacing regex loops.
- **Unified Policy Engine**: `assay-core`, `assay-cli`, and `assay-mcp-server` now share the exact same evaluation logic (`McpPolicy::evaluate`).
- **New Commands**: `assay policy validate`, `migrate`, and `fmt`.
- **Enforcement Modes**: `enforcement.unconstrained_tools: warn|deny|allow` for finer control over headless/legacy tools.
- **Scoped Refs**: `$ref` support within single policy documents (`#/schemas/$defs/...`).

### Changed
- **Runtime Consistency**: `assay mcp wrap` (proxy) and `assay-mcp-server` enforce the exact same rules as `assay coverage`.
- **Auto-Migration**: Legacy v1 policies (`constraints:`) are auto-migrated in-memory with deprecation warnings.

### Deprecated
- **v1 Constraints**: The `constraints:` syntax is deprecated and will be removed in Assay v2.0.0. Use `assay policy migrate` to upgrade.

### Fixed
- **JSON Casing**: Stabilized `structuredContent` vs `structured_content` in error contracts.
- **Symlink Resolution**: Fixed policy resolution issues on macOS `/tmp`.



### 🛠️ Autofix & Policy Packs
A major productivity release introducing automated self-repair (`assay fix`) and instant policy scaffolding (`assay init --pack`).

### ✨ Major Features
-   **`assay fix`**: Interactively repair configuration issues.
    -   **Automated Patches**: Fixes config errors, schema violations, and missing policies based on diagnostics.
    -   **Dry Run**: Preview changes before applying them.
    -   **Atomic Writes**: Cross-platform safe file updates (Windows/Linux/macOS).
-   **Policy Packs (`assay init --pack`)**:
    -   `default`: Balanced security (blocks RCE, audits sensitive ops).
    -   `hardened`: Maximum security (allowlist-only, strict args).
    -   `dev`: Permissive for rapid prototyping (logs warnings).

### 🛡️ Hardening
-   **Patch Engine**: Strict traversal prevents partial mutations during `remove`/`replace` operations.
-   **Module Cleanup**: Extracted shared logic to `assay-cli::util` for better maintainability.
-   **Windows Support**: Robust atomic file replacement strategy.

## [v1.4.1] - 2026-01-06

### 🩹 Consistency & SARIF Polish
Post-release hardening for Agentic Contract and SARIF compliance.

### 🛠️ Fixes
-   **Contract Consistency**: Internal severity normalization (`warning` -> `warn`) now applied strictly to exit code logic and CLI text output logic.
-   **SARIF**: `invocations.exitCode` now accurately reflects the CLI exit code (0/1/2).
-   **Contract**: Text output summary counts now strictly match JSON output counts.



## [v1.4.0] - 2026-01-06

### 🛡️ Agentic Security Edition
The "CI Gate" release. This major update transforms Assay into a comprehensive CI/CD guardrail for Agentic systems.

### ✨ Major Features
-   **`assay init`**: Interactive wizard that auto-detects your project type (Python/Node/MCP) and generates secure policy + CI config in < 5s.
-   **`assay validate`**: Dedicated CI command with strict exit codes (0=Pass, 1=Fail, 2=Error) and zero overhead.
-   **Agentic Contract**: `--format json` output is now strictly typed, stable, and designed for AI self-correction loops.
-   **GitHub Advanced Security**: `--format sarif` support for direct integration with GitHub Code Scanning.

### 📚 Documentation
-   **Overhaul**: Complete rewrite of `Quickstart`, `CLI Reference`, and `Architecture` guides.
-   **GetAssay.dev**: One-line install script and landing page sync.

## [v1.3.0] - 2026-01-06

### ✨ New Feature: `assay mcp config-path`
Simplified 1-step setup for Claude Desktop, Cursor, and other MCP clients.
-   **Auto-detection**: Automatically finds config files on macOS, Windows, and Linux.
-   **Generation**: Generates secure JSON snippets for your `mcpServers` config.
-   **Security**: Enforces policy file usage by default.

### 🛡️ Security Hardening
-   **Fail-Secure**: CLI now fatal-errors if specified policy file is missing (no insecure fallbacks).
-   **Policy**: clarifications on rate limit fields.
-   **Proxy**: Improved logging for unknown tool calls.

### 🐛 CI Fixes
-   **Python Wheels**: Fixed extensive artifact corruption issue in Release workflow (`release.yml`).
-   **Linting**: Strict `clippy` and `rustfmt` compliance across the board.

## [v1.2.12] - 2026-01-05

### 🩹 Fix
-   **README**: Fixed broken CI status badge (pointed to non-existent `assay.yml`).

## [v1.2.11] - 2026-01-05

### 📖 Docs Pages Update
-   **Index**: Aligned landing page with new "Vibecoder + Senior" positioning.
-   **User Guide**: Rewritten to focus on CI/CD, Doctor, and Python workflows (removed legacy RAG metrics noise).
-   **Consistency**: Unified messaging across README and documentation site.

## [v1.2.10] - 2026-01-05

### 📖 Documentation Refresh
-   **README**: Overhauled for "Vibecoder + Senior" audience.
-   **Guides**: Updated Python Quickstart and Identity docs.
-   **Consistency**: `assay-it` is now the canonical package name in docs.

## [v1.2.9] - 2026-01-05

### 🧹 Code Sweep
-   Removed redundant directories (`test-*/`, `assay-doctor-*`).
-   Refactored `doctor` module to remove verbose comments.
-   Zero fluff policy applied.

## [v1.2.8] - 2026-01-05

### 📚 SOTA DX Features
-   **Python Docs**: Added comprehensive docstrings to `assay.Coverage`, `assay.validate`, and `AssayClient` wrappers. IDEs will now show rich tooltips. (Google-style)
-   **Stability**: Added CLI verification tests for `assay init-ci`.

## [v1.2.7] - 2026-01-05

### 🩹 Formatting Fix
Patch release to verify `cargo fmt` compliance after `v1.2.6` refactoring.

## [v1.2.6] - 2026-01-05

### 🩹 Clippy Fix
Patch release to fix a stable-clippy lint `regex_creation_in_loops`.
-   **Performance**: Regex is now compiled once per doctor suite, not per policy.

## [v1.2.5] - 2026-01-05

### 📦 PyPI Metadata Fix (Real)
Updated `pyproject.toml` to explicitly use `assay-it` as the package name, ensuring `maturin` builds the correct wheel metadata for PyPI.
-   **Distribution Name**: `assay-it` (Final Fix)

## [v1.2.4] - 2026-01-05

### 📦 PyPI Package Rename
Renamed the Python SDK distribution package to `assay-it` to match the PyPI project name.
-   **Distribution Name**: `assay-it` (PyPI)
-   **Import Name**: `import assay` (Unchanged)

## [v1.2.3] - 2026-01-05

### 🩹 CI Stabilization
Patch release to resolve build pipeline issues.

-   **Fix**: Resolved artifact corruption in wheel generation (PyPI Release).
-   **Fix**: Corrected formatting in `doctor/mod.rs` to pass strict CI linting.

## [v1.2.2] - 2026-01-05

### 💅 Polish & Fixes
Strictness doesn't have to be unfriendly. This release polishes the "Strict Schema" experience.

-   **Friendly Hints**: When unknown fields are detected (e.g. `require_args`), Doctor now suggests the closest valid field ("Did you mean `require_args`?").
-   **Output**: `assay doctor` now correctly displays diagnostic messages in human-readable output (previously they were counted but hidden).
-   **Release Fix**: Removed legacy workflows to ensure smooth PyPI publishing.


## [v1.2.1-ext] - 2026-01-05

### 🩺 Smart Doctor (SOTA Agentic Edition)
Transformed `assay doctor` into a "System 2" diagnostic engine for Agentic workflows.

-   **Analyzers**:
    -   **Trace Drift**: Detects legacy `function_call` usage (recommends `tool_calls`).
    -   **Integrity**: Validates existence of all referenced policy/config files.
    -   **Logic**: Detects alias shadowing (e.g. `Search` alias hiding `Search` tool).
-   **Agentic Contract**:
    -   Output via `--format json` is strict, machine-readable, and deterministic.
    -   Includes `fix_steps` for automated self-repair.
    -   **Robust JSON Errors**: Even config parsing failures return valid JSON envelopes (when requested), ensuring Agents never crash on plain text errors.

### ⚠️ Breaking Changes (Strict Schema)
To prevent "Silent Failures" (phantom configs), we now enforce **Strict Schema Validation**:
-   **Unknown fields in `assay.yaml` or `policy.yaml` now cause a HARD ERROR.**
-   Previously, typos or incorrect nesting (e.g. `tools: ToolName:`) were silently ignored. Now you will see `E_CFG_PARSE` with "unknown field".
-   *Why*: Required for reliable Agentic generation and debugging.

### 🐛 Fixes
-   **Demo**: `assay demo` now generates canonical, schema-compliant policies.
-   **DX**: Restored `request_id` uniqueness check in trace client.

## [v1.2.0] - 2026-01-04

### 🐍 Python SDK (`assay-python-sdk`)
Native Python bindings for seamless integration into Pytest and other Python workflows.

-   **`AssayClient`**: Record traces directly from python code using `client.record_trace(obj)`.
-   **`Coverage`**: Analyze trace coverage with `assay.Coverage(policy_path).analyze(traces)`.
-   **`Explainer`**: Generate human-readable explanations of tool usage vs policy.
-   **Performance**: Built on `PyO3` + `maturin` for high-performance Rust bindings.

### 🛡️ Coverage Thresholds & Gates (`assay coverage`)
New `assay coverage` command to enforce quality gates in CI.

-   **Min Coverage**: Fail build if coverage drops below threshold (`--min-coverage 80`).
-   **Baseline Regressions**: Compare against a baseline and fail on regression (`--baseline base.json`).
-   **High Risk Gaps**: Detect and fail if critical `deny`-listed tools are never exercised.
-   **Export**: Save baselines with `--export-baseline`.

### 📉 Baseline Foundation (`assay baseline`)
Manage and track baselines to detect behavioral shifts.

-   `assay baseline record`: Capture current run metrics.
-   `assay baseline check`: Diff current run against stored baseline.
-   **Determinism**: Guaranteed deterministic output for reliable regression testing.

### Added
-   **`assay-python-sdk`** package on PyPI (upcoming).
-   `TraceExplainer` logic exposed to Python.

## [v1.1.0] - 2026-01-02

### Added

#### Policy DSL v2 - Temporal Constraints

New sequence operators for complex agent workflow validation:

- **`max_calls`** - Rate limiting per tool
  ```yaml
  sequences:
    - type: max_calls
      tool: FetchURL
      max: 10  # Deny on 11th call
  ```

- **`after`** - Post-condition enforcement
  ```yaml
  sequences:
    - type: after
      trigger: ModifyData
      then: AuditLog
      within: 3  # AuditLog must appear within 3 calls after ModifyData
  ```

- **`never_after`** - Forbidden sequences
  ```yaml
  sequences:
    - type: never_after
      trigger: Logout
      forbidden: AccessData  # Once logged out, cannot access data
  ```

- **`sequence`** - Exact ordering with strict mode
  ```yaml
  sequences:
    - type: sequence
      tools: [Authenticate, Authorize, Execute]
      strict: true  # Must be consecutive, no intervening calls
  ```

#### Aliases

Define tool groups for cleaner policies:

```yaml
aliases:
  Search:
    - SearchKnowledgeBase
    - SearchWeb
    - SearchDatabase

sequences:
  - type: eventually
    tool: Search  # Matches any alias member
    within: 5
```

#### Coverage Metrics

New `assay coverage` command for CI/CD integration:

```bash
# Check tool and rule coverage
assay coverage --policy policy.yaml --traces traces.jsonl --min-coverage 80

# Output formats: summary, json, markdown, github
assay coverage --policy policy.yaml --traces traces.jsonl --format github
```

Features:
- Tool coverage: which policy tools were exercised
- Rule coverage: which rules were triggered
- High-risk gaps: blocklisted tools never tested
- Unexpected tools: tools in traces but not in policy
- Exit codes: 0 (pass), 1 (fail), 2 (error)
- GitHub Actions annotations for PR feedback

#### GitHub Action

```yaml
- uses: assay-dev/assay-action@v1
  with:
    policy: policies/agent.yaml
    traces: traces/
    min-coverage: 80
```

#### One-liner Installation

```bash
curl -sSL https://assay.dev/install.sh | sh
```

### Changed

- Policy version bumped to `1.1`
- Improved error messages with actionable hints
- Better alias resolution performance

### Experimental

The following features are available but not yet stable:

- `assay explain` - Trace debugging and visualization (use `--experimental` flag)

### Migration from v1.0

v1.1 is fully backwards compatible with v1.0 policies. To use new features:

1. Update `version: "1.0"` to `version: "1.1"` in your policy files
2. Add `aliases` section if using tool groups
3. Add new sequence rules as needed

Existing v1.0 policies will continue to work without modification.

## [v1.0.0] - 2025-12-29
### Added
-   **Structured Logging**: `assay-core` now uses `tracing` for fail-safe events (`assay.failsafe.triggered`), enabling direct Datadog/OTLP integration.
-   **Protocol Feedback**: `assay-mcp-server` now includes a `warning` field in the response when `on_error: allow` is active and an error occurs, allowing clients to adapt logic.
-   **Documentation**: Added "Look-behind Workarounds" to `docs/guides/migration-regex.md`.

## [v1.0.0-rc.2] - 2025-12-28

### 🚀 Release Candidate 2
Rapid-response release addressing critical Design Partner feedback regarding MCP protocol compliance and operational visibility.

### ✨ Features
- **Structured Fail-Safe Logging**: Introduced `assay.failsafe.triggered` JSON event when `on_error: allow` is active, enabling machine-readable audit trails.
- **Fail-Safe UX**: Logging now occurs via standard `stderr` to avoid polluting piping outputs.

### 🐛 Fixes
- **MCP Compliance**: `assay-mcp-server` tool results are now wrapped in standard `CallToolResult` structure (`{ content: [...], isError: bool }`), enabling clients to parse error details and agents to self-correct.


### 🚀 Release Candidate 1
First Release Candidate for Assay v1.0.0, introducing the "One Engine, Two Modes" guarantee and unified policy enforcement.

### ✨ Features
- **Unified Policy Engine**: Centralized validation logic (`assay-core::policy_engine`) shared between CLI, SDK, and MCP Server.
- **Fail-Safe Configuration**: New `on_error: block | allow` settings for graceful degradation.
- **Parity Test Suite**: New `tests/parity_batch_streaming.rs` ensuring identical behavior between batch and streaming modes.
- **False Positive Suite**: `tests/fp_suite.yaml` validation for legitimate business flows.
- **Latency Benchmarks**: confirmed core decision latency <0.1ms (p95).

### 🐛 Fixes
- Resolved schema validation discrepancies between local CLI and MCP calls.
- Fixed `sequence_valid` assertions to support regex-based policy matching.

## [v0.9.0] - 2025-12-27

### 🚀 Hardened & Release Ready

This release marks the transition to a hardened, production-grade CLI. It introduces strict contract guarantees, robust migration checks, and full CI support.

### ✨ Features
- **Official CI Template**: `.github/workflows/assay.yml` for drop-in GitHub Actions support.
- **Assay Check**: New `assay migrate --check` command to guard against unmigrated configs in CI.
- **CLI Contract**: Formalized exit codes:
  - `0`: Success / Clean
  - `1`: Test Failure
  - `2`: Configuration / Migration Error
- **Soak Tested**: Validated with >50 consecutive runs for 0-flake guarantee.
- **Strict Mode Config**: `configVersion: 1` removes top-level `policies` in favor of inline declarations.

### ⚠️ Breaking Changes
- **Configuration**: Top-level `policies` field is no longer supported in `configVersion: 1`. You must run `assay migrate` to update your config.
- **Fail-Fast**: `assay migrate` and `validate` now fail hard (Exit 2) on unknown standard fields.

### 🐛 Fixes
- Fixed "Silent Drop" issue where unknown YAML fields were ignored during parsing.
- Resolved argument expansion bug in test scripts on generic shells.

## [v0.8.0] - 2025-12-27
### Added
- Soak test hardening for legacy configs
- Unit tests for backward compatibility
- `EvalConfig::validate()` method

### Changed
- Prepared `configVersion: 1` logic (opt-in)
