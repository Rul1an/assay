# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

### Added
- `assay evidence show --format json` includes `content_hash_scope`, supplied by the
  shared public `assay_evidence::crypto::id::content_hash_scope()` API. It describes
  the reader's hash inputs and separates event-file, run-root and archive integrity
  layers (#2777, #2785). It does not reconcile `manifest.algorithms`, authenticate a
  producer or establish evidence completeness.

### Fixed
- `assay-mcp-server` reports a pre-parse message-size refusal as JSON-RPC error
  `-32000`, with a null request id and `data.kind: transport_limit`, rather than a
  tool result. The session remains available for subsequent requests (#2779).
- `assay-mcp-server` classifies malformed `tools/call` envelopes and unknown tools
  as JSON-RPC `-32602` errors before dispatch, with distinct fixed error kinds and
  no reflected tool-name or argument values (#2776, #2780). These server changes
  also apply when that binary is packaged in an MCP bundle; they do not establish
  authenticated Claude or Codex host compatibility.
- The `assay` CLI's trace path measures string truncation in UTF-8 bytes and records
  escaped RFC 6901 field paths, including array indices. Trace verification reports
  stage-local truncation shape separately from missing prompts, where exact prompt
  coverage cannot be established (#2786, #2795, #2804).
- Trace storage updates a Step's content hash and truncation metadata together with
  its replacement content, preventing provenance from the previous value from
  surviving an upsert (#2787, #2805). This is the bounded storage repair; it does
  not introduce the separately planned trace-v7 schema.

### Changed
- Open the Assay 6.1 source line so post-`v6.0.0` minor-compatible public API work can declare
  a version increment that covers it. The workspace version, internal dependency declarations,
  both lockfiles, and the generated source-version surfaces now read `6.1.0`;
  `.github/assay-release-tag` and every published installation instruction stay on verified
  `v6.0.0` (#2789). This records a source-line state only: no `v6.1.0` tag, package publication,
  installability, or host proof is claimed.

## [6.0.0] - 2026-09-03

### Changed
- Open the Assay 6.0 source line for one bounded, already accepted breaking change: ADR-048.
  ADR-047's next-major step was already paid in `v5.0.0` (#2139) and is not part of 6.0. Published
  installation instructions and the install pin remain on verified `v5.5.2` until `v6.0.0` exists
  and its release artifacts have been verified (#2764).
- ADR-048: the claim-gate vocabulary `ClaimDecision { allowed, degraded, blocked }` and
  `ClaimKind { positive_existence, exhaustive_set, bounded_negative }` now lives once, in
  `assay_common::claim`. `assay_runner_schema::{ClaimGateDecision, CoverageClaimKind}` and
  `assay_evidence::{CodingAgentGateDecision, CodingAgentClaimKind}` are re-exports of those types
  under their former names, so existing imports keep resolving and the serialised spelling is
  unchanged and now pinned by tests. `assay-runner-schema` gains a normal dependency on
  `assay-common` (`default-features = false`). The decision tables stay in their crates. Code that
  bridged the two former enums with a `From` impl no longer compiles, because they are one type.

## [5.5.2] - 2026-08-31

Corrective patch release preparation. Published installation pins remain on
`v5.5.1` until the new release assets and packages have been verified. Existing
tags and published artifacts are not replaced.

### Fixed
- Update the locked `chacha20` dependency from yanked 0.10.0 to 0.10.2, which
  repairs upstream's use of an SSE4.1 intrinsic in an SSE2-gated backend. The
  dependency is reachable through `rand` and `object_store` in the evidence/MCP
  dependency graph (#2728, #2729). This is not a claim of a reproduced Assay
  crash or a newly discovered cryptographic weakness.
- Bind assembled release README platform coverage to the release version and
  reject duplicate coverage claims, including same-line duplicates (#2708,
  #2711). Windows tests decode the generated text as UTF-8.
- Keep the Claude plugin workflow safe to import and remove the inherited
  credential-storage override when preparing a fresh configuration namespace
  (#2690, #2727). This does not establish complete account isolation.

### Documentation And Verification
- The Claude plugin installation recipe includes both CLI and MCP prerequisites;
  RFC 8785 adequacy documentation separates three declared dependency rules from
  the wrapper control (#2723, #2666).
- The published-release harness exercises doctor preflight, allowed requests,
  unsupported wire requests and policy-record drift (#2670, #2669). A closed
  protocol-gate test binds accepted revisions to public support claims (#2671).

Authenticated Claude and Codex launch acceptance remains separate
post-publication work (#2194, #2684). Synthetic transcripts and successful
installation are not host-discovery or model-mediated invocation proof. MCP
`2026-07-28` remains unaccepted; this patch does not activate it.

## [5.5.1] - 2026-08-30

Recovery release for the unpublished `v5.5.0` attempt. It includes the changes
listed under 5.5.0 below. The `v5.5.0` tag remains unchanged: Windows README
packaging failed before GitHub Release creation and downstream package
publication. Published installation pins remain on `v5.4.0` until the recovery
release assets have been verified.

### Fixed
- Release archive README output uses explicit UTF-8 rather than the inherited
  stdout encoding. Regression tests exercise assembled-archive output under
  cp1252, ASCII, and UTF-8 in the existing Windows and Ubuntu release-asset
  contract job (#2705).

## [5.5.0] - 2026-08-30

Tagged release attempt; not published because Windows README packaging failed.
The following changes are retained in the v5.5.1 recovery release.

This version collects the post-`v5.4.0` main line: bounded evidence and
candidate-capture surfaces, generated release and Python-artifact contracts,
validation of model-mediated Claude transcripts, and stricter machine-readable
identity and policy interfaces. The MCP `2026-07-28` adapter remains behind its closed
gate; this release does not advertise or accept that revision. Install pins
and GitHub release assets stay on `v5.4.0` until this tag is published.
Authenticated model-mediated Claude host proof remains post-publication work
(#2194); transcript validation is not evidence that this journey has passed.

### Changed
- `assay evidence verify-privileged-mcp-action` still emits
  `assay.privileged_mcp_action.verify.report.v0` and keeps `profile` as the
  selected interpreter. The report now always projects `profile_selection`
  (`default` or `explicit`), `input_profile` (JSON `null` on frozen v0/v1),
  and `input_profile_status` (`undeclared_legacy`). Unknown in-namespace
  payload-schema findings retain that exact string on `observed_schema`.
  An in-namespace envelope type whose payload declares no schema omits
  `observed_schema` rather than republishing the envelope type. This does
  not add autodetect, migration, or a v0/v1 identity retrofit. `CHANGELOG.md`
  is the single deprecation announcement surface: a released reader must be
  announced deprecated here before the release that stops accepting it, and
  announcement and removal must not share a release. Between those releases,
  acceptance is supported only where a release test exercises that reader
  (#2574).
- Assay's GitHub Action consumer pin is a mixed Action migration to published
  `Rul1an/assay-action` v3.1.0, not a purely additive one. Completed
  zero-evidence runs now emit output `verified` as the literal `false`
  instead of an empty string. `sandbox-command` evidence now enters verify
  and `fail_on` lint, so a run can fail where the previous unverified sandbox
  route completed, and it writes a sandbox bundle into the workspace. The
  user-observable delta from Action v3.0.1 to v3.0.2 was not measured here
  (#2651).
- `assay-it` metadata, release wheel targets, and install docs share one
  `assay.python_artifact_matrix.v0`. That matrix is the sole mutable
  authority for CPython version, package, target, and tag. A plan job
  reads it and emits the wheels JSON plus `==X.Y.*` → X.Y / cpXY;
  release.yml and smoke consume those outputs (no hardcoded interpreter).
  Requires-Python is exact `==X.Y.*` bound to `cpXY` and to
  `Programming Language :: Python :: X.Y`. `support_bound` is derived
  from the declared wheels and that X.Y, not free prose. Requires-Python
  changed from `>=3.9` to `==3.12.*`. This does not break any previously
  working 3.9–3.11 install: those wheels never existed. Only the error
  becomes explicitly unsupported. The remedy is CPython 3.12. Pinning
  5.4.0 does not restore support. kernel-matrix `pull_request.paths` now
  include the remaining install-doc pages
  (`docs/guides/troubleshooting.md`, `docs/AIcontext/user-flows.md`) so
  those docs-only PRs start the hook; this does not claim every
  install-doc PR was already covered. Each wheels-job cell smokes the
  locally produced wheel (exactly one `.whl` whose name is the expected
  `{dist}-{version}-{tag}.whl`, no-index only-binary install, version +
  `assay._native`). Both macOS cells are native: x86_64 on
  macos-15-intel, arm64 on macos-15. There is no unsupported escape for a
  declared pair. The published-release golden-path harness digest for
  `.github/workflows/release.yml` was regenerated through the manifest
  owner refresh after that wheels-job change. This does not change
  published 5.4.0 files. The smoke contract witnesses the production
  `install_and_import` invocation; the smoke self-test spies that
  production call (importlib + monkeypatch); install_docs includes
  `docs/migration-v1.2.md` and active pip-install pages cannot be omitted
  (#2649).

### Added
- The RFC 8785 conformance exercise declares three near-miss rules in pinned
  `serde_jcs`: UTF-16 key ordering, ECMAScript-compatible number rendering,
  and Unicode string emission (Slice C). The wrapper replacement remains a
  separate control, not a fourth scored rule. This is not RFC certification,
  specification completeness, or full implementation adequacy.
- `assay project-enforcement-health` accepts a bounded, verified evidence bundle
  carrying exactly one typed `assay.sandbox.degraded` event and projects it as
  `observation: degraded`. Missing, duplicate, unknown, tampered, oversized, or
  contradictory active-health input fails closed with empty stdout. Bundle
  verification establishes integrity, not producer authenticity or enforcement
  efficacy (#2641).

- `assay mcp manifest candidate --from-observed ... --out ...` exports a
  deterministic, non-approved review candidate only from a complete,
  unambiguous `assay.mcp_manifest_observed.v0` artifact. `assay mcp manifest
  promote --candidate ... --source ... --out ...` rereads the exact observed
  bytes, reconstructs and compares every candidate field, then creates a new
  strict declared-v0 baseline without overwriting an existing file. A
  candidate is not approval, provenance, a safety verdict, or proof that an
  operator understood the change (#2655).

- `assay policy resolve --input PATH --format json` emits one
  `assay.policy.resolved.v0` document after the same load and schema-compile
  path as `assay policy validate`. The whole-policy digest is
  `McpPolicy::policy_digest()` under `jcs:mcp_policy`. Malformed YAML keeps
  typed `E_POLICY_PARSE` on stderr with empty stdout. Validate stdout stays
  `assay.run_summary.v1` (#2510).

- The Claude plugin workflow now runs its bounded model session with
  `--output-format stream-json --verbose` and classifies the transcript through one
  validator, `classify_model_mediated_call`, that fixture replay and the live path both
  call. `model_mediated_tool_call=pass` requires exactly one decide tool_use whose name
  is exactly `mcp__assay__assay_policy_decide` or exactly
  `mcp__plugin_assay_assay__assay_policy_decide` in an assistant-role envelope whose
  input is exactly the pinned probe the live prompt asks for, exactly one
  matching non-error tool_result in a user-role envelope after it, a payload typed against
  the server's contract with every `matches` member a non-empty string, a later assistant
  message quoting a value the model could not have taken from its own request, and exactly
  one terminal result envelope after that turn reporting `subtype: success` with
  `is_error` absent or the literal boolean `false` — checked by identity at both
  `is_error` sites, so `0`, `"false"` and other stand-ins are refused. The pass detail
  reports `observed_route=project` or `observed_route=plugin`. All of those envelopes must share one non-empty session id. Absence
  of an accepted decide name, including a wrong Assay tool that is not decide, stays
  `not_exercised` and keeps the invoked name in the detail. Malformed, oversized,
  duplicated, out-of-order, wrong-session, incomplete and error transcripts stay
  `unavailable`. Process exit is never
  the evidence, and an incomplete observation never becomes clean (#2632, #2688, child of #2194).

  The record path uses its own **allowed** probe, separate from the blocked probe that
  proves policy-root resolution. `assay-mcp-server` maps a policy denial to MCP
  `isError: true`, so a denied probe can never yield an accepted transcript; a self-test
  guard fails closed if the two are ever re-coupled. A denied decision is therefore
  evidence about policy routing, never about model-mediated tool use.
  The live prompt is built by one function and asks for the field the allowed decision
  actually carries (`reason`); `matches` exists only on a denial. The fake session refuses
  a prompt that has dropped that output contract, so injected fixture bytes can no longer
  mask prompt drift.

- The CLI JSON identity guard now follows writers to rows as well as rows to writers. Every
  production file under `cli/commands` that serializes JSON through the six issue idioms must be
  named by a `cli-documents` writer/namer, an `unnamed-documents` producer, or an explicit
  stale-checked opt-out. Nineteen previously unrecorded CLI JSON emits were added as unnamed
  document rows without changing production commands, including the second emit forms in
  `evidence lint` (SARIF, not the run-report `sarif` row), `evidence list` (`list_all` vs
  `list_for_run`), and `assay profile update` (`ASSAY_PROFILE_PERF_JSON`, not `profile show`).
  Fourteen files were opted out after reading the emit path. The command-tree walk admits one
  safe ASCII component at a time, rejects a symlink root and nested symlinks fail-closed, and
  never uses `DirEntry::path()`. No schema, runtime, or CLI output changed (#2555).
- `capture_candidate.py` records a candidate's observations over the opaque cases and
  writes `assay.privileged_mcp_action.candidate_capture.v0`. It takes no manifest and no
  expectations, so a candidate that escapes its process bounds finds no answers on that
  host. `score_candidate.py --capture` scores that artifact on a host that never ran the
  candidate; `--entrypoint` still captures and scores in one process, and both paths call
  the same capture builder and the same scorer, so a given candidate yields byte-identical
  run records either way. A capture that does not bind the pack is refused before any
  comparison, exit `2`, and no run record is written. `report.v0` gains optional
  `implementation.id` and `implementation.image`, present only together, so an existing v0
  record stays valid. A capture is an observation artifact, not a verdict, attestation,
  sandbox proof, or independent reproduction; a capture host can fabricate observations, so
  separating the phases removes the oracle from the hostile host without authenticating what
  that host produces (Rul1an/assay-tunnel-experiments#199).
- `conformance/implementations.json` is a static, fail-closed registry of candidate
  images addressed as `name@sha256:<64 hex>`. Authorship is a typed object (`human`,
  or agent kinds with model and prompt strategy). Required CI calls the same stdlib
  validator, which reuses the shared regular-file reader and JSON parser. A digest
  addresses bytes; registration does not authenticate a publisher or prove safety,
  reproducibility, independence, or conformance
  (Rul1an/assay-tunnel-experiments#198).
- `docs/architecture/CLI-JSON-IDENTITIES.md` records which `assay.<segments>.vN` identities
  ship and which of them are CLI JSON documents. `cli_json_identities.rs` fails the build when
  source and record disagree, including for the nine documents that carry no identity at all
  and which no source scan can see; three of those twelve were found only by an independent
  read, after two separately written inventories missed them (#2484, #2167).
- `assay describe [path...]` walks the clap command tree so a caller can ask
  for the top-level surface and descend. Node identities are the existing
  shipping constants, not a second registry. Global `--quiet` /
  `--non-interactive` stay absent: `NO_COLOR` is already honoured and `watch`
  is the only interactive command (#2178).
- A pinned, model-only MCP `2026-07-28` wire vocabulary validates required request metadata,
  `resultType`, cache hints, `server/discover` shapes, and `-32022` error data without advertising
  or accepting that revision (#2481).
- A complete MCP `2026-07-28` stateless server adapter exists in-process behind a
  closed gate. Public stdio still refuses `_meta: 2026-07-28` with `-32022`,
  keeps `server/discover` at `-32601`, and does not advertise or accept that
  revision (#2482).

### Changed
- Observed, candidate, and declared MCP manifest files now share one inclusive
  1 MiB read and write ceiling. Existing `--declared-mcp-manifest` startup is
  therefore stricter: a file above that limit is refused instead of being
  materialized and parsed. Duplicate JSON members and unknown
  `field_digests` keys are also refused through the shared strict loader
  (#2655).
- One strict bounded loader now reads every hostile JSON input in the conformance corpus.
  `strict_json.load_strict_object` calls the existing regular-file reader and JSON parser and
  adds the two bounds they did not carry: nesting depth, scanned before `json.loads` sees the
  text, and the JSON number domain. `validate_run_record.py` reads through it rather than
  keeping its own reader, which changes one diagnostic's wording and no exit status.
  `conformance/implementations.py` exposes `validate_image_reference`, and `_validate_row`
  calls it instead of matching the image pattern a second time.

- `assay_check_sequence` answers the sequence-rule language by calling
  `assay_core::sequence_eval` and mapping the record into its published JSON.
  The MCP copy of the rule is gone (#2227, #2228).
- `TraceExtent` and session-finding notes state that extent makes no fidelity claim:
  `complete` must not be read as "nothing is missing" (#2422). A producer-declared
  `coverage` field on the finding waits for a planned major.
- `tool_description_integrity` hashes `input_schema` over canonical RFC 8785 bytes through
  `assay_core::mcp::jcs`. Pins recorded against the prior compact-JSON preimage may need
  regeneration even where a schema is unchanged; key reordering no longer reads as a mutation
  (#2245).

### Fixed
- `assay sandbox --enforcement-health` now requires `--enforce-net` at clap
  validation, matching `--probe-enforcement`. The previously accepted combination
  requested a v1 artifact without requesting its Landlock producer and wrote
  nothing (#2635).
- Evidence-vocabulary CI structurally rejects a required command after an
  unconditional `exit`/`return`, or when that command is the skipped operand
  of `false &&` / `true ||`. A short-circuit skip does not make later lines
  unreachable. `true && cmd` still executes in Bash, so it is a canonical-form
  miss rather than a reachability bypass; this is not a runtime execution
  witness. Tracked-file NUL scanning is content-first: NUL fails closed unless
  the path matches a declared POSIX, case-sensitive, segment-bound path
  class AND the bytes match that class's expected magic. A header prefix
  alone is not a binary. Vacuous path classes fail. No `run_root` semantic
  change (#2362).
- Named MCP request-envelope projection now fails closed when `params` is missing or is not a JSON
  object. These cases report stable `fallback_projection_missing_params` or
  `fallback_projection_invalid_params` checks, exit `2`, and serialize `binding.digest` as `null`
  instead of publishing a digest over synthetic or unsupported input (#2595).
- `assay init` stdout contract tests now separately pin the deterministic empty-project prefix
  and the ordered `--from-trace` closing block (#2254).
- Published-release golden path and `examples/privileged-action-gate/run.sh` verify the
  live-produced denial-observation bundle with `--profile-version v1`. The verifier default
  stays `v0`. No product, policy, or process-exit change.

## [5.4.0] - 2026-08-19

This release collects the post-`v5.3.0` main line: privileged-mcp-action v1
denial recognition, Assay-owned proxy application codes with a v1 deny
observation, outer-fallback telemetry, the h2 advisory refresh, published-release
golden-path verification, and runner registration-token recovery. Install pins
and GitHub release assets stay on `v5.3.0` until this tag is published. The
verifier default stays `v0`. No policy, forwarding, or process-exit semantic
change is claimed.

### Added
- Readers accept the exact privileged-mcp-action v1 denial marker (v1 schema /
  `-31999` / `assay-proxy`) through one shared classifier. Default profile
  selection stays `v0`; historical v0 corpus remains byte-exact (#2508, #2520).

### Changed
- Assay-owned MCP proxy application codes move out of the JSON-RPC reserved band:
  unsupported `-31997`, failed `-31998`, denied `-31999`. The opt-in deny observation is
  `assay.denied_call_observation.v1` and binds the same `-31999` code. An upstream reserved
  `-32042` (MCP URL elicitation) still relays value-equivalently and does not mint an Assay
  observation. Live produce verifies with `--profile-version v1`; the verifier default stays
  `v0`. No policy, forwarding, or process-exit change (#2509, #2521).
- Telemetry follow-up for the #2391 outer-fallback surface: `tool_call_crash`
  was removed; outer dispatch failures emit `tool_execution_error`. Remaining
  `tool_call_start` / `tool_call_done` / `tool_call_timeout` events stay
  value-free (no caller tool or policy strings). `tool_decision` is unchanged
  and still out of this note's scope (#2402, #2480).
- `h2` is updated past RUSTSEC-2026-0258 (#2506).
- CI proves the published-release golden path, and the product-support views
  describe that published evidence (#2513, #2514).
- Auto-recovery runner registration tokens stay fresh; timed-out recovery
  phases fail closed (#2478).

## [5.3.0] - 2026-08-18

This release collects the post-`v5.2.0` main line: fail-closed machine stdout,
MCP policy ingest through one env-parsed byte ceiling that keeps the published
`ServerConfig` shape minor-compatible, and `initialize` echo of Cursor
`2025-06-18`. Install pins and GitHub release assets stay on `v5.2.0` until
this tag is published.

### Added
- The JSON failure process contract now pins the explicit gate-command set (`run`, `ci`,
  `coverage`, and `validate`) and drives both machine and default output modes (#2177).
- Legacy `assay coverage --format json` argument failures now publish the existing
  `assay.run_summary.v1` diagnosis on stdout. Text mode stays operator-only and `--input` mode
  keeps writing `coverage_report_v1` to its requested file (#2177).
- `assay ci --format json` now publishes its authoritative `assay.run_summary.v1` report on stdout
  for completed gates and early pipeline failures while preserving `summary.json`; default text
  mode keeps stdout clear (#2177).
- A stable JSON `startup_failure` diagnosis is emitted on stderr for enforcing-proxy policy,
  manifest, and establish-budget startup failures while MCP stdout stays empty and recovery stays
  independent of log level (#2163).
- `E_EVIDENCE_CONTRACT` is registered for a readable evidence bundle that violates its declared
  format contract (`ErrorClass::Contract` / `Contract*`). It stays distinct from recorded-value
  mismatch and from unreadable I/O or archive failures. `assay evidence verify-privileged-mcp-action`
  constructs it for typed `Contract*` stage-1 failures (#2165).
- `E_EVIDENCE_LIMIT_EXCEEDED` is registered for a typed evidence-verifier ceiling refusal
  (`ErrorClass::Limits` / `Limit*`). Verification stopped before reaching a verdict, so it asserts
  nothing about the bundle's content. Constructed by `verify-privileged-mcp-action` for reachable
  `Limit*` codes (#2165).
- `E_EVIDENCE_PATH_REJECTED` is registered for a typed archive-path refusal
  (`ErrorClass::Security` / `Security*`). The command-neutral classifier covers both
  `SecurityPathTraversal` and `SecurityAbsolutePath`. Command-level synthetic drive covers
  the reachable traversal code in `writer_next/verify.rs`; AbsolutePath remains a
  non-claim for that stage-1 verifier file (#2165).
- `verify-privileged-mcp-action` and `CliFailure` share one `anyhow::Error` classifier:
  typed `VerifyError` is authoritative; untyped I/O (missing file, directory/`EISDIR`)
  is `E_EVIDENCE_UNREADABLE` only when no verifier code is present. The privileged
  command stays on profile-report v0. `findings.detail` may retain the caller argv
  path. Unreadable `next_step` is shell-free caller-argv via `ReasonCode::next_step`
  (concrete JSON `Run argv` with `--` and the caller path), not a second
  remediation. Other owned codes stay prose. `stage1_fail_report` requires a
  `ReasonCode`. The six owned evidence codes are binary-owned in
  `PROFILE_EVIDENCE_REASON_CODES` (#2165).
- `E_EVIDENCE_PROFILE_INVALID` is registered and constructed for a stage-1 pass whose privileged
  MCP action profile verdict is invalid. It is not a bundle defect and carries no claim or source
  class (#2165).

### Changed
- `assay-mcp-server` now treats MCP `2025-06-18` as a supported legacy revision and echoes it
  on `initialize`. The supported set is `2024-11-05`, `2025-06-18`, and `2025-11-25`. Unknown
  requests still fall back to `2025-11-25`; this does not implement or advertise `2026-07-28`
  (#2448).
- Six machine-document stdout branches now use the shared fail-closed writer instead of raw
  `println!`: `evidence adapt-skill-scan`, `evidence attest`, `evidence capture-skill-supply-chain`,
  `evidence project-skill-bom`, and both `project-otel` projections. A requested document that
  cannot be delivered returns the registered output-write exit `3` rather than aborting the process.
  Open-reader bytes, `--out` file behaviour, and successful exits are unchanged (#2441).
- `assay-mcp-server` prefixes every line of its top-level human error chain, so a caller-supplied
  path containing a newline can no longer place an unprefixed JSON line on stderr that a
  line-oriented consumer reads as a second `startup_failure` event. The machine event itself is
  unchanged: still path-free, still emitted once, still carrying its reason code and next step
  (#2436).
- `summary.json` and machine-readable early-failure summaries now apply the existing JSON
  record-sink safety pipeline to failure `message` text. Secret shapes and terminal controls are
  neutralized without truncating record content or changing Assay-owned contract fields, including
  exact machine-readable recovery argv in `next_step` (#2168).
- `assay doctor --format json` and `assay run --format json` return exit 3 when
  writing the machine document to stdout fails, including `BrokenPipe`. A
  partial or absent document is not a clean success. They no longer abort with
  SIGABRT (134). The mapping is the existing output-write policy used by
  `supply-chain-conformance` (#2263).
- `assay evidence show --format json` now emits `assay.run_summary.v1` with
  `E_EVIDENCE_CONTRACT` for a typed `Contract*` format-contract failure. Classification
  comes from the shared `reason_code_for_evidence_error` classifier. The command still
  publishes only CONTRACT, INTEGRITY, and UNREADABLE; LIMIT, PATH, and PROFILE remain
  unpublished on this path (#2412).
- The built-in stdio MCP policy tools now fail closed on outer dispatch errors
  and timeouts. Caller `arguments.on_error` no longer selects fail-open;
  responses use fixed value-free `E_INTERNAL` / `E_TIMEOUT` messages and
  `isError: true`. Clients following the former gateway example must remove
  that argument. Suite `settings.on_error` remains an `assay run` setting
  (#2391). Outer dispatch failures emit `tool_execution_error`; `tool_call_*`
  events no longer copy caller-controlled tool or policy strings. The separate
  `tool_decision` evidence event retains its existing redaction contract.
- The five advertised MCP policy tools now read local policy files through one
  inclusive byte ceiling (default 1,000,000, override
  `ASSAY_MCP_MAX_POLICY_BYTES`) before parse or cache insertion. Exactly the
  limit is accepted; one extra byte returns `E_LIMIT_EXCEEDED`. The ceiling is
  independent of the JSON-RPC `ASSAY_MCP_MAX_BYTES` message limit and is not a
  `ServerConfig` field, so the v5.2.0 public struct shape stays minor-compatible
  (#2453). Operators with previously accepted larger local policy files must
  shrink the file or set an explicit bounded override. This does not bound
  parser nesting, YAML aliases, proxy startup policy, manifests, trust
  policy, or CLI readers (#2389).
- `assay_policy_decide` now rejects non-mapping roots and canonical or mixed name-policy
  documents (`allow`, `deny`, `tools`, or those markers mixed with root `blocklist`) with
  `E_POLICY_PARSE` before cache insertion. Those inputs previously returned a false clean
  allow or ignored the canonical fields. This is an intentional hardening change, not
  "unsupported with no impact." Route full, argument-aware policy evaluation to
  `assay_check_args`. Migration: keep a root-`blocklist` file for the name-only tool, and
  move `tools.allow` / `tools.deny` documents to `assay_check_args` (#2386).

  ```yaml
  # assay_policy_decide compatibility dialect only
  blocklist:
    - dangerous_tool
  ```

  ```yaml
  # full McpPolicy for assay_check_args — do not pass this to assay_policy_decide
  tools:
    deny:
      - dangerous_tool
  ```

### Fixed
- `assay doctor` text output now passes caller-derived interpolations (target
  path, parse-error detail, suite, and diagnostic message) through the existing
  `render_safe(Sink::Stdout, …, usize::MAX)` pipeline so ESC/CSI/OSC8/BEL cannot
  paint the terminal. Assay-owned labels stay byte-stable, and the JSON channel
  remains serializer-owned (#2265).

## [5.2.0] - 2026-08-14

This release hardens the evidence verifier and makes the Linux monitor's declared observation
surface match what it actually attempts to attach. It also consolidates CI setup and release-tool
pinning without changing the CLI or evidence wire schemas.

### Added
- The Linux monitor declares mode-aware terminal outcomes for its probe inventory and now attempts
  the compiled `sendto` and `sendmsg` tracepoints on every supported run. Runner coverage labels
  remain count-derived, while CLI observation health remains attach-derived (#187, #2339, #2344,
  #2345).
- Monitor summaries and Runner capture notes account for datagram events that carried no usable IP
  peer, so a silent peer list is distinguishable from a parser drop (#2344, #2347).
- Public docs record a one-host measured io_uring CONNECT bound: the syscall tracepoint was blind,
  while cgroup `connect4` observed and blocked the operation on the measured host. This is a bounded
  measurement, not a cross-kernel support claim (#2346).

### Changed
- Privileged-action evidence verification applies one shared claim-ceiling fold across the profile
  and side-effect verifier, rejects malformed decision streams, and requires one-to-one allocation
  between decisions and audit records. Inputs accepted only because of the previous fail-open paths
  can now be rejected (#2352, #2355).
- Current docs and agent context define `run_root` as SHA-256 over its canonical tuple. Rekor/RFC
  6962 inclusion proofs remain identified separately, and a scoped vocabulary guard prevents
  conflating the two (#2222, #2357).
- CI uses shared Rust setup across the required, perf, fuzz, runner and kernel lanes, with centrally
  pinned Cargo plugins in the required and split-wave lanes. The change reduces duplicated setup
  while preserving the three required contexts and fail-closed security gates (#2224, #2287-#2335).
- Current packaging docs distinguish the project-scoped Claude Code/Cursor MCP manifests from the
  equivalent Codex TOML recipe; no Codex JSON manifest or additional distribution artifact is
  claimed (#2351).

### Fixed
- Machine-summary stdout failures now share one fail-closed policy across early `run`/`ci`
  failures, typed CLI failures, and `policy validate`: an undelivered JSON document returns the
  registered infrastructure exit `3` without panicking or becoming a config exit (#2439).
- The Linux monitor no longer aliases the `dedup_open_paths` setting with another CONFIG-map key,
  renders raw connect destinations as `ip:port`, and fails closed when the loaded eBPF program set
  drifts from the declared inventory (#2337, #2338, #2341).
- CI invokes Cargo subcommand plugins with their required subcommand argv and isolates advisory
  databases, hook-created Git fixtures and mutation-hook cleanup state (#2318, #2328, #2331,
  #2353).

## [5.1.0] - 2026-08-11

This release turns the install-to-verifiable-evidence path into an agent-facing product surface.
It adds project and plugin installation for the golden path, gives non-interactive callers
structured failure contracts at the commands they use to set up and inspect that path, and removes
an MCP protocol claim the server did not implement. The consumer-visible compatibility corrections
below may require CLI, MCP, evidence-input or policy-pin migration.

### Added
- Project-scoped MCP manifests for Claude Code, Cursor and Codex start the separate
  `assay-mcp-server` binary with `--policy-root .`, and a contract test pins the five production
  tool names rather than only their count (#2158).
- A shared `assay-golden-path` skill guides agents from installation to verifiable evidence and is
  generated byte-identically for the supported project roots. Drift, vocabulary and hostile-path
  tests guard the shipped workflow (#2175, #2180, #2182, #2183, #2187).
- A Claude Code marketplace package bundles the MCP configuration, skill, references and assets so
  the same workflow can be installed as `assay@assay` (#2192).
- `assay policy validate`, `assay init`, `assay doctor` and `assay evidence show` now publish
  machine-readable JSON outcomes for their previously silent failure paths. The contracts include
  stable reason identities, actionable next steps and the command-specific result data needed by
  non-interactive callers (#2198, #2240, #2207, #2204, #2262).
- `E_EVIDENCE_INTEGRITY` distinguishes hostile or damaged evidence content from argument, baseline
  and replay failures, while `E_EVIDENCE_UNREADABLE` identifies evidence that cannot be opened or
  read. Both use shared mappings across evidence consumers (#2213, #2262).
- Run and validation JSON documents now name their `schema` and `schema_version`, and failing
  `assay run --format json` diagnostics are emitted on stdout for machine consumers (#2151, #2169).
- `assay evidence show --format json` now reports whether verification was enabled or disabled via
  its additive `verify_mode` field (#2262).

### Changed
- The MCP server no longer advertises unimplemented protocol revision `2026-07-28` or responds to
  `server/discover`. Clients pinned to that revision must negotiate `2025-11-25` or `2024-11-05`;
  the public `MODERN_PROTOCOL_VERSION` constant remains available but deprecated (#2267).
- `assay doctor` now exits `2` for configuration-class failures. This changes JSON runs with an
  error-severity configuration diagnostic from `0` to `2`, and unloadable-config failures without
  `--fix` from `1` to `2`. An unloadable config now retains that class under `--fix` when no repair
  is available, a repair is declined or previewed, or the repair leaves it unresolved (#2247,
  #2209).
- MCP `tool_pins` now require 64-character lowercase hexadecimal hashes, and schema identity uses
  RFC 8785 canonical JSON bytes. A policy carrying a malformed pin now fails to load. Re-record
  otherwise valid pins created by older versions after upgrading or calls can be denied with
  `E_TOOL_DRIFT` (`P_TOOL_DRIFT` in decision events) (#2239, #2268).
- Machine-readable next-step guidance for configuration and policy parse failures now renders an
  explicit argument vector rather than a shell command string, avoiding ambiguous quoting. Policy
  validation recovery also adds the previously missing `--input` flag (#2198, #2204).
- `assay-mcp-server enforcement-sarif` now fails closed instead of skipping malformed non-empty
  NDJSON lines and rejects input above 16 MiB. Streams accepted by 5.0.0 can therefore fail and must
  be repaired or reduced before conversion (#2197).

### Fixed
- The MCP server no longer advertises protocol revision `2026-07-28` while omitting required result
  fields. The public compatibility constant shipped in 5.0.0 remains available but is explicitly
  deprecated and no longer controls negotiation (#2267).
- `assay doctor` now derives text and JSON exit classes through the same decision function,
  including failed repair and unloadable-config paths (#2247).
- Tool-pin schema hashes are validated at load and computed from canonical JSON bytes, preventing
  spelling differences from changing identity while rejecting malformed pinned values (#2239,
  #2268).
- Release and dependency-security guardrails that previously existed without reaching the required
  aggregate gate are now wired into it, and the dependency job invokes the pinned audit binary it
  installs (#2236, #2238).

## [5.0.0] - 2026-08-08

A major spent deliberately rather than paid by accident. Two of these breaks were
waiting for a major to exist: the ineffective-assertion default was phased to one
in #1949, and the `Payload` variant was deferred out of #2122 for the same reason.
The other three rode it rather than motivating one of their own. Grouping them is
the point: after this, a new event kind, a new failure mode and a new rule step
are all minors.

### Breaking
- **An assertion that cannot fail is now refused at load.** `assay run` and `assay ci` stop before
  execution when a config carries an assertion no trace could fail, such as
  `trace_must_call_tool` with `min_calls: 0` or `trace_must_not_call_tool` with an empty tool name.
  The escape hatch is `--allow-ineffective-assertions`, and the refusal names it, along with the
  test, the assertion index and the responsible field. Diagnostics stay value-free and never echo
  suite content.

  This is the last step of the route #1949 set out: `assay validate` has reported these as a
  warning since #1983, `--deny-ineffective-assertions` made the refusal available as an opt-in, and
  a major is where a config-breaking default belongs. The opt-in flag is replaced rather than kept
  alongside its inverse, so there is one name for the axis.

  Library callers are affected too, and by construction: `LoadOptions` now carries
  `allow_ineffective_assertions` (default `false`), so `load_config` and every
  `..Default::default()` acquire the refusal. A caller that needs the old behaviour asks for it
  explicitly with `allow_ineffective_assertions: true`.

  **Migration:** run `assay validate` first. It reports the same set, using the same code, so it
  tells you exactly what will be refused before you upgrade.
- `Payload::Unknown` is removed. It read like a catch-all while matching only the literal tag
  `"Unknown"`, which no producer emits, so it promised forward compatibility the type did not
  provide (#2123). Unrecognised kinds were already a deserialisation error and still are; forward
  compatibility lives on the wire, where `EvidenceEvent::payload` stays a raw `serde_json::Value`.
- `assay_evidence::types::Payload` is `#[non_exhaustive]` and carries `SessionFinding`, so admitting
  a future event kind is a minor rather than a major (#2126).
- The 27 public error enums are `#[non_exhaustive]`, so a new failure mode is a minor (#2140).
- Sequence rules read calls rather than names. `SequenceRule`'s tool fields are `CallSelector`,
  which deserialises from a bare string exactly as before, so **every existing YAML parses
  unchanged**; the object form is new capability. A step may now constrain the call's arguments:

  ```yaml
  - type: never_after
    trigger:   { tool: bash, args_match: { command: "\\.aws/credentials" } }
    forbidden: { tool: bash, args_match: { command: "^curl\\b.*-d" } }
  ```

  This is what makes "credential read followed by egress" writable at all: in the recorded
  demonstration that motivated it, all three calls are `bash` and only the arguments separate them.
  `evaluate_rules` takes `&[SequenceCall]` instead of `&[String]`, and `assay-metrics` no longer
  discards `args` before evaluation (#2124).

### Added
- `PayloadSessionFinding::new` and `EVENT_TYPE` are added so a producer does not restate the field
  order or respell the tag. No command emits a session finding yet; the kind is admitted to the
  format and wiring a producer is separate work (ADR-047, #2105).

## [4.0.0] - 2026-08-06

The first major since the 3.x line, for one field. Everything else here is
gate-hardening and defect repair.

### Breaking
- `MetricResult` carries a fifth public field, `exercised`, so a struct-literal
  construction that compiled against 3.38.0 no longer does
  (`error[E0063]: missing field 'exercised'`). Every constructor —
  `pass`, `fail`, `unstable`, `not_applicable`, `not_exercised` — is unchanged,
  so callers using those are unaffected. The field is what lets a consumer tell
  a metric that ran and passed from one that returned 1.0 without evaluating
  anything, which was previously indistinguishable (#2068).

  This break reached `main` unnoticed because the `cargo-semver-checks` job is
  in a `workflow_dispatch`-only workflow and has never run on a pull request.
  Tracked as #2088.

### Added
- Metrics and assertions now report whether they were actually exercised, not
  only whether they passed. Assertion-based verification calls this a companion
  cover: an assert with zero attempts is a coverage hole, not a pass.
  - `Exercised { NotApplicable, NotExercised, Exercised }` on `MetricResult`,
    with 22 sites across `assay-metrics` reclassified from a vacuous `pass(1.0)`
    (#2068).
  - A test whose named metric evaluated nothing is reported in the console
    summary, in `run.json`'s `warnings`, and in the `ci` job summary, under
    `W_METRIC_NOT_EXERCISED`. It never changes a status, a count or an exit
    code (#2083).
  - The same for the `assertions:` surface, under
    `W_ASSERTION_NOT_EXERCISED`. A `trace_must_not_call_tool` naming a tool the
    agent was never offered held on every trace and could not have failed;
    availability is now the antecedent, and an unrecorded tool list reports
    nothing rather than guessing (#2085).
  - The baseline treats an `Exercised → NotExercised` drop between runs as a
    coverage regression. Without it a metric that stopped running made the
    baseline look *better*, because its score rises to 1.0 (#2082).

### Fixed
- Every SARIF report pointed its help links at a domain that is not ours.
  `docs.assay.dev` does not resolve, and `assay.dev` belongs to an unrelated
  third party; six rule `helpUri`s and the driver `informationUri` shipped in
  every report, and a seventh reference printed to the terminal. They now point
  at a lint rules reference under `docs/lint/`, with anchors pinned explicitly
  rather than derived, and a test asserts every emitted fragment resolves
  against the committed page. The repo's link checker could not have caught
  this: it triggers only on `docs/**`, so a change to Rust source never fires
  it, it skips anything starting with `http`, and it reads only files changed
  in the pull request (#2091).
- The lint truncation disclosure named the cap only once the cap had fired.
  Since `max_results` defaults to 5000 there is always a ceiling, so a clean
  report was indistinguishable from an unbounded one. `appliedCap` is now
  declared on every run and `droppedCount` only when a drop occurred, which is
  the split ratified across four emitters in the SARIF envelope RFC and which
  this producer's own defect report is cited in (#2091).
- A test's score is no longer whichever metric ran last. `final_score` was
  assigned unconditionally per metric, and `semantic` is fifth of thirteen, so a
  semantic test scoring 0.87 reported 1.0 in `run.json` and SARIF — a number
  from a metric that was never asked to run (#2068).
- Five `--format` arguments accepted any string and fell through to a default.
  `assay doctor --format totally-invalid` printed the text report at exit 0;
  `policy generate --format jsom` wrote a YAML policy into whatever path was
  named. The check that should have caught them compared against a hand-kept
  table that never listed them, and now derives the set from the command tree
  (#2086).
- The Linux compile gate returned 0 on failure, so it could not catch the
  `cfg(target_os = "linux")` errors it exists for (#2076).
- The docs bot opened pull requests whose required checks could never run,
  because `create-pull-request` signs them with the default `GITHUB_TOKEN` and
  GitHub does not trigger workflows for that token. The drift is now checked on
  the change that causes it (#2080).
- `normalize_severity` downgraded an unrecognized severity to `note`, which
  could turn a failing run into exit 0; `assay-core` carried a second copy, and
  that was the one reaching GitHub Code Scanning (#2025, #2033).
- `decide_exit` classified by string prefix, so one missing trace produced two
  different exit codes. The exit class is now a table the registry owns (#2024).

### Changed
- ADR-046 records that the two reason-code registries stay separate, after
  measuring that the collision #2010 was filed on does not exist: the two SARIF
  producers write different `tool.driver.name`, so GitHub keys them in separate
  namespaces (#2028).
- `docs/architecture/REASON-CODE-VOCABULARIES.md` inventories every
  reason-code-shaped vocabulary by the artifact field it writes to, with four
  machine-checked blocks (#2026, #2027).


## [3.38.0] - 2026-08-04

### Changed
- Releases now refuse to build over an open blocker. A `release-blocking`
  label plus a step in the release workflow's contract job: if the milestone
  being released has an open issue carrying that label, the run fails and names
  the issues before any artifact is built. There is deliberately no override
  input — removing the label or moving the issue to a later milestone are
  recorded decisions on the issue itself, which is better provenance than a
  workflow toggle. A release with no matching milestone is not blocked. This
  exists because the gate on #1949 lived only in a sentence and lapsed when
  3.37.0 shipped over it (#1985).

### Fixed
- The canonicalizer is now checked against RFC 8785 rather than against itself.
  31 edge-case vectors, cross-validated with an independent implementation in
  another language, covering number reformatting, both ES6 exponent boundaries,
  UTF-16 code-unit key ordering and the absence of Unicode normalization. Every
  content id, mandate id and bundle run root is a sha256 over these bytes, so a
  divergence would have made those digests irreproducible by any conforming
  implementation while every internal test stayed green (#1982).
- An `assertions:` entry carrying a key the type does not define is now rejected
  at parse time, naming the offending key, instead of being dropped in silence.
  Where the intended field had a default the dropped key left a check that could
  not fail: the documented `max_calls: 0` "must NOT use a forbidden tool"
  example inverted into "must be called at least once", because `max_calls` was
  discarded and `min_calls` defaulted to 1. Rejection holds through the config
  loader, which collects unknown keys elsewhere and outside strict mode only
  warns about them. **This turns previously green configurations red, by design:
  an unknown key in an assertion is either a typo or a feature that does not
  exist** (#1961).
- Every `assertions:` example in the documentation now loads. None of them did:
  five of the seven documented types had no implementation, the two that existed
  were shown with field names that did not, the architecture example nested them
  under a top-level `policies:` block the loader refuses at `configVersion: 1`,
  and the example labelled "must NOT use a forbidden tool" asserted the
  opposite. The catalogue is rewritten from the enum, and a test now loads every
  fenced example, requires every shipped variant to be documented, and requires
  every type listed as unimplemented to actually be rejected (#1960).
- `assay validate` now sweeps `assertions:` instead of stepping over them. The
  exemption tested whether a test carried a **non-empty** assertion list, not an
  **effective** one, and nothing examined the assertions afterwards — so a single
  assertion that could not fail cleared both gates at once: the `expected:` check
  was skipped because assertions existed, and the assertions were never looked
  at. A suite could be swept clean while asserting nothing. Effectiveness is
  decided by the same code the evaluator runs, so there is one definition of
  "cannot fail" rather than a static one and a runtime one that drift. An
  unrecognized `expect` spelling — which selected *expect failure* and inverted
  the assertion — is now caught in the same sweep (#1949).
- An `assertions:` entry that cannot check anything no longer reports as a pass.
  Thirteen shapes evaluated to nothing or to a check that could not fail for any
  input — an `args_valid` without `test_args`, a `sequence_valid` written
  against the unread `test_trace` field or carrying no usable `regex`, a
  `tool_blocklist` whose `blocked` list was absent, empty, wrongly typed or
  partly unreadable, `min_calls: 0`, an empty `sequence` with
  `allow_other_tools: true`, an empty `tool` name, a `max` at the largest
  representable bound, and others — and each now reports
  `E_ASSERT_INEFFECTIVE` naming the responsible field. Separately, `expect` was
  compared by exact equality to `"pass"` at three sites, so any other spelling
  silently selected *expect failure* and inverted the assertion; unrecognized
  values are now rejected. **This turns previously green configurations red, by
  design: they were green because they checked nothing** (#1949).

### Added
- `assay.tool_decision_surface.v0` records now carry a typed `correlation`
  basis: a valid W3C `traceparent` from `_meta` is retained verbatim as a
  propagated claim, a broken carrier is typed `malformed_trace_context` with
  its bytes dropped, and a stateless record is typed `none` — never silently.
  Additive to the shipped v0 shape; records from producers up to and including
  3.37.0 carry no `correlation` field (#1955).

## [3.37.0] - 2026-08-02

### Added
- Observe the MCP protocol era on import and carry it through wire-profile
  normalization, with an era-parity corpus pinning how protocol-era results are
  concluded. The event shape is unchanged (#1929, #1934, #1935).
- Bounded MCP OTLP/JSON decoder with pre-retention ceilings, so an oversized or
  deeply nested payload is refused before it is retained rather than after
  (#1931, #1944).

### Fixed
- Reject vacuous and ambiguous `expected` assertions. An `args_valid` policy
  that asserts nothing now fails at config load instead of passing every
  response, and shared JSON Schema `$defs` are prepared once for the preflight,
  cached, one-shot and MCP paths so they cannot drift apart. JSON Schema is
  compiled hermetically: an external `$ref` is refused by an explicit denying
  retriever, with jsonschema's default features off as a second layer. Only
  same-document and embedded references are supported (#1948).
- Separate immutable MCP replay from live drift in CI (#1947).
- ProtoJSON ingest correctness: int64 coercion, omitted-message defaults and
  nested null key defaults now follow the spec.

### Notes
- Not a general schema satisfiability solver. `expected` is judged vacuous only
  for policies the rule fully understands; anything else keeps its specific
  diagnosis ("not enforced by this evaluator", "must be a list").

## [3.36.0] - 2026-07-29

### Added
- Bind new `evidence-bundle/v1` attestations to the SHA-256 digest of the exact bundle archive.
  The new `verify_envelope_signature` API returns an explicitly artifact-unmatched state; the
  deprecated `verify_envelope` compatibility shim keeps its published 3.x signature. Only
  `verify_attestation_for_bundle` can establish that the signed subject and predicate match a
  verified bundle. Existing v0 attestations must be re-issued, and `assay evidence attest` now
  refuses caller-supplied `--predicate` data because every v1 predicate field is derived from the
  verified archive.
- Separately publish `privileged-mcp-action-v0-candidate.3` as a conformance prerelease: a
  deterministic 14-case clean-room pack with checksums and GitHub artifact attestation. This
  improves the independent-reproduction surface; it does not itself satisfy the non-author
  reproduction gate.

### Changed
- Declare Rust 1.89 as the MSRV for all public crates and enforce that floor against the locked
  workspace graph and all Linux-host targets in required CI. Repository development remains pinned
  to Rust 1.96, and the eBPF nightly remains a separate internal build toolchain.
- The stdio MCP server now negotiates its two tested legacy handshake revisions. Requests for
  `2024-11-05` or `2025-11-25` receive that exact revision; other string values receive the
  latest supported legacy revision, `2025-11-25`. Missing or structurally incomplete initialize
  parameters fail with JSON-RPC invalid params. This does not claim MCP `2026-07-28` support: the
  modern `server/discover` and per-request metadata contract remains separate.
- Verify-only bundle paths no longer retain the decompressed event stream after validating it,
  reducing peak memory without changing the verified result. Reader APIs that expose events still
  retain that content for their callers.

### Fixed
- **Fail-closed correction (bundle verification, migration-visible).** The verifier now rejects
  every bundle `BundleWriter` refuses to emit. Five shapes previously verified: an empty bundle, an
  inconsistent `source` across events, a `source` that is not a URI, and a blank line in
  `events.ndjson` — which was counted toward `event_count` while contributing no content hash, so a
  padded bundle satisfied the count check and the `run_root` chain by having them measure different
  things. Bundles carrying any of these verified yesterday and do not today. No bundle produced by
  this repository is affected; all 26 in the tree were checked before the change. Third-party
  producers targeting the format should compare against `StreamRule`, now public at
  `assay_evidence::bundle::StreamRule`.
- Refuse IPv6 CIDR policy rules before monitor attachment instead of silently applying only the
  supported IPv4 subset.
- Pin, audit and consume the evidence fuzz lockfile through its own CI lifecycle, and require
  warning-free public rustdoc including the stable `assay-registry/oidc` feature surface.

## [3.35.0] - 2026-07-27

### Added
- Publish the open `privileged-mcp-action/v0` evidence profile, its 13-vector conformance corpus,
  and typed importer and verifier support for one classified privileged MCP `tools/call`. The
  profile keeps policy decisions, caller-visible outcomes and optional observations distinct. It
  does not establish provider-side effects, generic identity, policy correctness, whole-action
  trust or a scalar score.
- Aim the evidence-bundle fuzz target at the evidence-chain verifier and add deterministic
  properties for chain order, truncation, digest mutation and fail-closed error classification.
  Required CI runs the property suite; a path-triggered and nightly lane runs bounded fuzz smoke
  with a pinned seed corpus.
- New reason code `E_REPLAY_LIMIT_EXCEEDED` (exit 2) for a replay bundle refused by an ingest
  ceiling during bounded ingest, before replay execution. Previously such a refusal was reported
  as `E_CFG_PARSE`, which is a malformed-input finding and sends a reader off to fix the
  producer. A ceiling establishes only that a configured budget was exceeded; the read stopped
  there, so whether the bundle is otherwise valid is unresolved, and adjusting the budget or
  supplying a smaller bundle is a legitimate response. Backward
  compatible: a new registry string under the existing `reason_code_version` 1, registered in
  `docs/architecture/SPEC-PR-Gate-Outputs-v1.md` §5.1, so consumers branching on
  `(reason_code_version, reason_code)` fall back as the spec already requires. The refusal
  carries no verdict, and its message names only the configured ceiling.

### Changed
- Apply the complete configured ingest limit set before materialization across verified and
  unverified evidence reads, manifest peeks, lint, stdin verification, push including
  `--no-verify`, object-store pull and replay bundles. Exact limits are accepted; limit plus one,
  decompression expansion, excessive lines, events, paths and JSON depth fail closed with typed
  limit classifications.
- Make the stdio MCP authorization boundary explicit. Any configured `ASSAY_AUTH_*` variable now
  fails startup before protocol I/O, and token-like values in `initialize` grant no authority and
  are neither logged nor used as identity. This does not add HTTP/OAuth support and does not change
  ProxyEnforce's explicit local policy caller input.
- **Fail-closed correction (`assay sandbox`, migration-visible)**: a `--policy` that is named but
  cannot be loaded is now fatal.
  The run exits 2 with `E_POLICY_LOAD_FAILED_UNENFORCEABLE` and executes nothing, where it
  previously printed a warning and continued under the built-in `mcp-server-minimal` pack.
  `--fail-closed` does not create this obligation and does not change the outcome; naming a
  policy does. Substitution remains in place where no `--policy` was given and the documented
  default applies. An invocation that relied on the old fallback now fails instead of running
  under containment the operator did not choose.
- `assay mcp-server` no longer emits `meta.certified` or `meta.partner` in the `initialize`
  result. Both were unconditional literals returned on every successful handshake, including
  sessions that failed authentication under the default permissive mode, and neither carried a
  basis a reviewer could check. `serverInfo.version` is now derived from the crate version
  instead of the hand-written `0.4.0`, which no build produced.
- **Fail-closed correction (`assay sandbox`, migration-visible)**: a named policy with non-empty
  `extends` is refused before execution. Pack resolution is unsupported, so accepting those entries
  previously made declared policy composition disappear silently. `assay sandbox --profile` now
  emits an empty `extends` list instead of references to unresolved packs.

The two sandbox corrections close cases where requested enforcement could not be honored; they do
not remove a supported enforcement contract. They ship on the `3.x` line as correctness and
integrity fixes rather than a new major version. Operators that relied on permissive substitution
must remove the explicit `--policy`, supply a loadable policy without unresolved `extends`, or
expect exit 2 with no command execution.

### Fixed
- Publish clean-room conformance packs as prereleases that are explicitly ineligible for GitHub's
  software `Latest` pointer. Installers, the embedded Action and runner maintenance also reject a
  non-`vX.Y.Z` latest tag before constructing a CLI asset URL.
- Keep Assay RC and beta releases out of the official MCP Registry. GitHub prereleases remain
  available for testing, but neither their automatic release event nor a manual registry dispatch
  may publish a prerelease version that would sort above the latest stable server. Registry
  publication also executes a version- and digest-pinned `mcp-publisher` rather than an unverified
  binary from the upstream `latest` release.
- Reject line breaks in explicit embedded Action versions before writing step outputs, preventing
  multi-line input from adding attacker-chosen Action outputs while preserving published tags such
  as `v2.1`. Two-component historical tags now retain their download identity while verifying the
  installed binary against its three-component version (`v2.1` -> `2.1.0`). The published major
  aliases retain the same guarantee (`v1` -> `1.1.0`, `v2` -> `2.12.0`).
- Sandbox policy documentation described a filesystem rule shape the loader has never
  accepted (`- path:` mappings with `read`/`write` keys, against a schema of plain strings).
  Copying an example verbatim used to produce a warning and now produces a hard failure, so
  the reference, the CLI page and the security guide now describe the narrower contract the
  sandbox runtime actually applies. The documented exit codes 3 and 4 for policy problems are
  also corrected: no code path emitted them, and both cases are configuration errors, which
  is 2.

## [3.34.0] - 2026-07-19

### Added
- Add `ASSAY-W005`, an evidence lint rule that flags approval bases declaring an opaque or
  unrecognized retained view (`encrypted`, or any value the reviewer cannot read), so content-review
  claims over them cap at incomplete. A present-but-empty or non-string view fails closed rather than
  being silently skipped, and integrity verification never lifts the verdict. Reserves the
  `encrypted` retained-view vocabulary (key holder, salted plaintext commitment, preimage
  content-type, payload location, opacity reason); no emitter exists yet, the reservation keeps
  imported or future records on a known shape.
- Bind the approval-artifact retention digest into `assay.enforcement_decision.v0`, carrying the
  retained view and its artifact digest alongside the decision.
- Bind kernel socket events to their originating cgroup in the runner, so network events attribute to
  the process that made them.

### Fixed
- Make the GitHub Release publish step idempotent: if a release for the tag already exists (for
  example a prior partial run left an empty release), attach the assets and refresh metadata instead
  of failing, so built artifacts and the crates.io/PyPI publish are no longer stranded.
- Adapt the registry to the `x509-cert` 0.3.0 accessor API.

## [3.33.0] - 2026-07-05

### Added
- Add the opt-in `assay.denied_call_observation.v0` carrier for caller-visible MCP proxy denials.
  It records the tool name, target digest when classification can bind one, structured proxy-deny
  fields, and a response-line digest, while keeping caller observations separate from
  `assay.enforcement_decision.v0` verdict records.
- Add `ASSAY-W004`, a bundle-index-backed evidence lint rule for enforcement-attribution overreads.
  It flags denied-call observations that lack a bound deny decision, and observations contradicted by
  a bound allow decision, without treating the lint finding as proof of enforcement correctness.

## [3.32.0] - 2026-07-04

### Added
- Add the `gateway-evidence-replay` workspace crate: a deterministic offline replay verifier for
  retained gateway-path evidence, with `gateway-path.v0` contract fixtures and a four-bundle demo. It
  verifies only the retained path facts and explicitly does not claim provider honesty, model-output
  truth, policy compliance, or end-to-end action trust.
- Add skill supply-chain evidence commands to the Assay CLI:
  - `assay evidence capture-skill-supply-chain`
  - `assay evidence verify-skill-supply-chain`
  - `assay evidence project-skill-bom`
  - `assay evidence adapt-skill-scan`
  The flow captures skill supply-chain facts, verifies the retained carrier, projects CycloneDX SBOM
  evidence, and adapts scanner output to SARIF without turning those facts into a trust score.
- Add gateway replay Gate-D demo docs and implemented-standards references, including the standalone
  `gateway-evidence-replay` ecosystem pointer.

### Changed
- Harden the delegated runner proof layer: canonical eBPF build provenance, attested delegated proof
  packs, content-addressed proof acceptance, stricter hostile-input limits, and self-tests for the
  proof consumer path.
- Simplify and harden CI workflows: preserve queued bpf-host runs, follow artifact redirects safely,
  reduce workflow hot-path cost, archive obsolete experiment workflows, and consolidate ADR-025
  nightly evidence into one sequenced informational workflow while preserving the readiness artifact
  contract during the transition.
- Update public discovery metadata (`CITATION.cff`, `README.md`, `llms.txt`, and PyPI keywords).

### Fixed
- Strip C1 terminal controls in render-safety handling.
- Harden skill capture symlink containment.
- Fix the eBPF LSM emit path to write current `MonitorEvent` records, with attach-smoke coverage for
  ABI skew.
- Resolve eBPF artifact paths from Cargo JSON messages instead of filesystem guessing.
- Close shell-injection risk in MCP registry publish tag resolution and tighten workflow token
  permissions.

## [3.31.1] - 2026-06-27

### Changed
- Refactor-only source hygiene release: split remaining handwritten Rust hotspots across CLI MCP
  commands, MCP proxy enforcement, evidence tests, registry supply-chain helpers, runner kernel/redaction
  tests, and tool-decision truth helpers so every handwritten Rust source file stays below 600 LOC.
- Add `scripts/ci/review-hotspot-loc-under-600.sh` as a reusable review gate for the hotspot threshold.

## [3.31.0] - 2026-06-24

### Added
- `assay.coding_agent.evidence_pack.v0` evidence primitive in `assay-evidence`: a typed payload for one
  coding-agent run (declared scope, observed effects, per-surface coverage, source class, non-claims) plus a
  `coding_agent_evidence_event(...)` helper that sets the hard `content_hash`. Facts only: no verdict, no
  effect-sufficiency, no policy decision (those stay downstream). `network` is required in the declared scope
  and in coverage; observed absence stays explicit, never a missing field. (#1754)

## [3.30.0] - 2026-06-23

### Added
- `assay-canonical` crate: strict JSON parse (rejects duplicate object keys and non-finite constants),
  RFC 8785 (JCS) canonicalization, content-id digests, registered set-paths, and the semantic-digest
  profile. First published in this release. (#1737, #1741)
- `EvidenceEvent` carries an additive, optional `semantic_digest` + `digest_profile` pair — a soft
  correlation/equivalence overlay alongside the hard `content_hash`, computed via `assay-canonical`
  (RFC 8785). It is never included in `content_hash`, never on the verify/admission path, and never
  substitutes `content_hash`/`mandate_id`. Absent by default (omitted from serialization). (#1750)

### Changed
- `assay-evidence` routes JCS and content-id digests through `assay-canonical` (byte-identical; goldens
  unchanged). (#1739)
- `assay-canonical::Error` is `#[non_exhaustive]` and exposes a typed `Error::DuplicateKey` carrying the
  offending key, distinct from `Error::Parse`. (#1741)

### Security
- Bump `quinn-proto` to 0.11.15 for RUSTSEC-2026-0185 (remote memory exhaustion from unbounded
  out-of-order stream reassembly). (#1742)

## [3.29.0] - 2026-06-18

### Added
- `assay registry supply-chain-conformance` now supports the `dsse` provenance kind: it verifies a local
  DSSE-wrapped in-toto/SLSA provenance envelope against caller-supplied pinned Ed25519 key material
  (STANDARD-base64 SPKI DER), entirely offline, via the existing `assay-registry` verifier — no new
  cryptography. The descriptor gains `provenance.payload_type` (must be `application/vnd.in-toto+json`),
  `provenance.envelope_path`, and `provenance.trusted_key_path`; the latter two resolve relative to the
  descriptor file and reject absolute paths, `..`, URLs, and symlink escape. The carrier schema and
  `assay.supply_chain_conformance.input.v0` are unchanged (additive fields). Well-formed-but-failing
  evidence (wrong key, tampered payload, subject mismatch, or a Rekor/timestamp/consistency/witnessing
  requirement) yields a not-clean carrier at exit 0, never a magic pass; descriptor/path/parse errors
  exit non-zero. The keyless `sigstore_bundle` path remains deferred. It still asserts no supply-chain
  safety, ecosystem trust, Sigstore trust, Rekor inclusion, issuer identity, policy approval, compliance,
  or runtime integrity.

## [3.28.0] - 2026-06-17

### Added
- `assay registry supply-chain-conformance` emits the `assay.supply_chain_conformance.v0` carrier from a
  local input descriptor (`assay.supply_chain_conformance.input.v0`), running the existing
  `assay-registry` supply-chain producer offline over caller-supplied inputs. It performs offline checks
  and reports carrier status; it does not assert supply-chain safety, policy approval, compliance,
  Sigstore trust, Rekor inclusion, issuer identity, or artifact runtime integrity. The descriptor is
  strict (unknown fields/variants are rejected, not ignored). v1 maps `none` and `unsupported`
  provenance (exercising the pinning, expected-digest, and policy dimensions); the signature-bearing
  `dsse` / `sigstore_bundle` paths are modeled but deferred to a follow-up (rejected with a clear error).
  A carrier is emitted at exit 0 even when `policy_result` is `incomplete`/`fail`; a missing/unreadable/
  malformed descriptor exits non-zero.

## [3.27.0] - 2026-06-16

### Added
- Offline Sigstore-keyless supply-chain verification for MCP packs, composed into the
  `assay.supply_chain_conformance.v0` carrier with orthogonal per-dimension statuses: Fulcio
  certificate chain against pinned roots (ECDSA P-256 and P-384), Fulcio identity (SAN + OIDC issuer),
  DSSE/PAE signature, in-toto subject-digest binding, and Rekor v2 offline inclusion under pinned
  verifier material. A new `not_checked` status distinguishes deliberately-unverified dimensions
  (timestamp freshness, log consistency, witnessing) from absent or failed ones. Fully offline: no
  network, no live transparency-log lookup. (#1701–#1710)
- Render-safety pipeline for rendered outputs (control-strip → redact → bound → sink-encode) across the
  Assay CLI sinks (console, `run.json`, SARIF, JUnit), with `assay.render_safety_conformance.v0` and a
  redaction receipt; redaction precedes bounding so a secret cannot survive as a truncated prefix. Proxy
  credential-boundary conformance (`assay.token_passthrough_conformance.v0`) shows a consumed inbound
  auth value is not re-emitted on outbound headers, body, or env. (#1691–#1694)

### Changed
- OWASP MCP Top 10 mapping: MCP01 and MCP04 promoted to **Strong (scoped)**. The mapping now has no
  Partial rows — Strong-or-better across all ten risks, with MCP01, MCP04, and MCP09 explicitly scoped
  to the evidence Assay verifies and carrying explicit coverage limits. This is not a claim that the
  OWASP MCP Top 10 is solved or eliminated. (#1700, #1711)

## [3.26.0] - 2026-06-14

### Added
- `assay-mcp-server enforcement-sarif`, projecting an `assay.enforcement_decision.v0` NDJSON stream
  into a SARIF 2.1.0 report for the GitHub Security tab. Only `deny` records become results (level
  `warning`); `allow` and non-enforcement records are skipped, and the projection reads only the
  sanitized fields the record already exposes (tool name, action class, reason, drift state), never
  raw arguments or targets. Reads stdin and writes stdout when paths are omitted.
- A copy-paste pull-request gate workflow template and a runnable, offline privileged-action example
  under `examples/privileged-action-gate`: an agent's `tools/call` runs through the enforcing proxy,
  the per-call decisions are projected to SARIF, and the PR fails when any privileged action is
  denied. The conformance signal stays out of the gate.
- Publishing of the MCP registry `server.json` via GitHub OIDC on release, so the registry entry
  tracks each published version.

### Fixed
- The `enforcement-sarif` projection now nests the finding under `locations[].physicalLocation` with a
  nested logical location, instead of placing `logicalLocations` directly on a SARIF result. The
  previous shape was rejected by GitHub code scanning whenever a report carried at least one deny, so
  a deny never reached the Security tab; a zero-deny report uploaded cleanly, which is why it was
  missed. A regression test now forbids the rejected shape.

## [3.25.0] - 2026-06-13

### Added
- Pre-call manifest-establish for the enforcing proxy (P61e Increment 2). When a `tools/call` would
  be denied solely because no current complete `tools/list` was observed for the tool, the proxy runs
  one bounded, proxy-originated re-list against a single total deadline, then re-decides on the
  effective observation and acts on that verdict. It never relaxes a gate: ambiguous observations, a
  missing baseline, and real digest drift stay denied, and a failed or timed-out re-list leaves the
  deny standing. The journey is emitted as a separate `assay.manifest_establish.v0` carrier (establish
  path + run outcome, never the allow/deny verdict) under `--manifest-establish-out`, with a
  `--manifest-establish-budget-ms` operator flag (default 5000).
- `assay.tool_annotation_conformance.v0`, a carrier comparing the server's untrusted declared tool
  annotations (`readOnlyHint` / `destructiveHint`) against Assay's own observed call classification.
  Emitted per `tools/call` under `--tool-conformance-out`, read from the same effective observation as
  the verdict, with an `observation_basis` (`complete` / `incomplete`) that keeps an unobserved
  manifest honest rather than reporting a false "undeclared". It is descriptive and orthogonal: a
  mismatch is never a verdict and never gates the call, and the allow/deny verdict stays in
  `assay.enforcement_decision.v0`.
- Pinned producer contracts for downstream consumers: golden fixtures for `assay.manifest_establish.v0`,
  `assay.tool_annotation_conformance.v0`, and a combined `assay.combined_carrier_acceptance.v0`
  fixture, each regenerated from the real producer builders and proving the verdict / journey /
  conformance carriers stay non-correlated.
- The `assay monitor` output line shapes are pinned by a pure, platform-independent
  `format_monitor_event` plus a contract test, so a change to the scraped lines is caught at the
  producer.

## [3.24.0] - 2026-06-12

### Added
- Enforcing-proxy policy decision point (P61e-c), an explicit opt-in `assay-mcp-server proxy-enforce`
  mode that decides each `tools/call` before forwarding. It is fail-closed by construction and runs
  three gates in fixed precedence: a caller-allowance gate, a credential-scope gate (c2) that denies
  when the declared upstream credential does not cover the action's required scope, and a manifest
  drift gate (c3) that requires both an approval-time baseline and a current complete observation and
  denies when the invoked tool's digest drifted since approval. Only a call clearing every gate is
  forwarded; there is no observe-only forwarding and no allow path without a current complete manifest.
  - It is not an authorization server and makes no grant decision of its own.
  - An allow is the decision to forward; it is not proof the call was delivered or that a side effect
    occurred (a transport failure surfaces to the caller, never as a delivery claim).
  - It decides per call; it does not reason about multi-step or sequential behaviour.
- `assay.enforcement_decision.v0` per-call evidence record (P61e-d): a deterministic record emitted for
  both allow and deny, carrying the decision, the precedence-pinned reason, `fail_closed`, the
  `drift_state`, and the credential alias (never the token or scopes). It records the policy decision
  only and carries no `forwarded` / delivery field.
- PDP golden corpus: an in-repo deterministic truth table over `enforce.rs::decide` covering every gate
  outcome, with reason precedence pinned and the emitted record shape asserted per case — the oracle the
  decision logic is regression-locked against.
- Canonical `assay.enforcement_decision.v0` contract fixture regenerated from `decision_record`, so a
  downstream consumer can vendor the exact producer output rather than a hand-authored mirror.

### Changed
- Supply-chain and public-artifact hardening: scheduled supply-chain posture, an HMAC-based trusted
  sanitizer layer, RUSTSEC advisory triage (`RUSTSEC-2026-0176`/`-0177` documented as not reachable,
  pending the pyo3 0.29 migration), and a `--locked` from-source smoke install.

## [3.23.0] - 2026-06-10

### Added
- MCP upstream manifest-observation proxy mode (`assay-mcp-server proxy --upstream-command <cmd>
  [--upstream-arg <a>]…`). An explicit, opt-in stdio proxy that sits in front of one upstream MCP
  server, forwards only the session handshake, `ping`, and the `tools/list` /
  `notifications/tools/list_changed` operations, observes the upstream `tools/list` read-only, and
  (with `--mcp-manifest-observed-out`) emits `assay.mcp_manifest_observed.v0` with honest completeness
  (`complete` / `partial` / `unknown` / `not_observed` / `ambiguous`, never read as clean when the
  observation was incomplete). A non-allowlisted method such as `tools/call` is rejected with a
  distinct proxy-originated error and is never forwarded upstream. A separate
  `--proxy-observation-health-out` records how complete the observation was, kept out of the manifest
  artifact (which stays the exact shape a consumer gates on). Spec:
  `docs/reference/mcp-upstream-proxy-mode.md`.
  - Does not support tool execution through the proxy.
  - Does not enforce `tools/call` policy.
  - Does not classify maliciousness.
  - Does not support HTTP upstreams.
  - Does not support multiple upstreams.

## [3.22.0] - 2026-06-10

### Added
- Tool-decision surface (`assay.tool_decision_surface.v0`): the MCP proxy now records each observed
  `tools/call` as a structured per-call decision, the privileged in-application actions kernel and
  network enforcement cannot see (a deploy key added, a workspace member invited). Each record
  carries the server identity, the rule-based privileged-action classification, a projected target
  (sensitive ids hashed under per-field domains, raw args and secrets never stored), the policy
  decision, and the response. The load-bearing rule travels in the shape: a tool returning success is
  the provider's assertion (`side_effect_asserted`), never proof (`side_effect_verified` stays false
  without independently checked audit evidence). Three rule-based classifiers ship
  (`github_deploy_key`, `slack_add_member`, `workspace_admin`); no model or judge decides a
  classification, and an unknown tool is `observed_unknown_tool`, never read as clean. Spec:
  `docs/reference/tool-decision-surface.md`.
- Credential-scope evidence: each classified privileged tool decision carries
  `action.required_scope`, derived deterministically from the action category (never from arguments),
  so a consumer can ask whether the credential alias the action used was appropriate. Credentials are
  declared metadata and observed aliases, not verified provider grants; no token introspection. Spec:
  `docs/reference/credential-scope.md`.
- Side-effect receipt spec (`docs/reference/side-effect-receipt.md`, spec + fixtures, experiment):
  an honesty ladder for privileged side effects (`asserted` -> `observed_confirmed` -> `verified`)
  and a binding contract. `verified` never means Assay queried the provider; it requires an
  independently imported provider audit record (`assay.provider_audit_record.v0`) whose binding Assay
  recomputes from committed bytes via canonical JCS. No producer/verifier yet.

### Removed
- The deprecated top-level command shims `assay discover`, `assay kill`, `assay tool`,
  `assay generate`, and `assay record` were retired. They had been hidden and printed a
  deprecation warning since the command-grouping pass; use the canonical paths instead:
  `assay mcp discover`, `assay mcp kill`, `assay mcp tool`, `assay policy generate`, and
  `assay policy record`. The underlying behavior, flags, output, and exit codes are unchanged.

### Added
- `assay sandbox --probe-enforcement` (with `--enforce-net`): runs a self-probe before the
  workload that, from inside the enforcing ruleset, attempts one connect to an ephemeral denied
  port. Only a proven real block (EACCES and the harness listener never reached) writes the
  `probe` block into `enforcement_health.v1` (`active` + probe). A probe that does not prove a
  block is reported and never silently dropped, and never fails the run. Weak signals (timeout,
  ECONNREFUSED, ENETUNREACH) never count as a block.

## [3.21.0] - 2026-06-10

### Added
- Landlock TCP-connect egress enforcement for `assay sandbox` (`--enforce-net`, requires
  `--enforce`): builds a combined FS+NET Landlock ruleset that allows only the explicit TCP
  ports in `net.allow` and denies all other TCP connects, applied via `restrict_self` in the
  enforcing child. A non-expressible network policy fails closed before spawn. With
  `--enforcement-health <path>` it writes the `assay.enforcement_health.v1` artifact (`active`
  when the ruleset is applied, `failed` with a machine-readable reason otherwise); a requested
  artifact that cannot be written is a command failure. FS-only sandboxing is unchanged.
- `assay doctor --format json` now carries a top-level `schema` id (`assay.doctor_report.v0`), making
  the report self-describing so a future field-shape change is an explicit version bump rather than
  silent drift. Additive; existing fields unchanged.
- `assay.enforcement_health.v1` carrier (types + committed fixtures only, no producer yet) for the
  Landlock TCP-connect port-allowlist enforcement domain. An explicit version bump from
  `assay.enforcement_health.v0` (left untouched; consumers read both additively): `status` is
  `active`/`failed` only (no `not_applicable`, no `absent` — presence means requested), `probe` is
  always present and `null` when no real-block probe ran, and `failure.reason_code` is a
  machine-readable enum. Fixtures: `crates/assay-cli/tests/fixtures/enforcement_health/v1/`.
- Landlock TCP-connect port-allowlist compile target (`assay_policy::tiers::compile_landlock_net`,
  types + tests only, no sandbox applies it yet). Compiles an explicit TCP-connect port allowlist
  and fails closed on every Landlock-inexpressible shape the policy can represent: IP/CIDR rules,
  negative/deny rules, host/wildcard destinations, and port 0, each with a machine-readable reason.
  The eBPF tier compiler is unchanged.
- Host-capability proof gate (CI): changes under `crates/assay-cli/src/diagnostics/` now require a
  validated `workflow_dispatch` run of the `host-capability-proof` workflow on the PR head SHA
  (event, SHA, conclusion, and workflow validated via the Actions API; doctor JSON read from the
  run artifact). The checker validates presence and JSON type of the Landlock capability fields,
  never their values. Contract: `docs/reference/runner/host-capability-proof.md`.

### Fixed
- `assay monitor` no longer exits 0 when a requested `--enforcement-health` artifact cannot be
  written. A consumer reads a missing artifact as "not requested" (absent), so an active run whose
  artifact write failed would have been misread as making no enforcement claim; the command now exits
  with an infra error instead. The fail-closed abort paths (attach failure) already exit non-zero and
  are unchanged.
- Diagnostics now read the Landlock ABI from the canonical `landlock_create_ruleset(NULL, 0,
  LANDLOCK_CREATE_RULESET_VERSION)` syscall instead of `/sys/kernel/security/landlock/abi_version`,
  which does not exist on mainline kernels and produced a false-negative `net_enforce` on real hosts
  (e.g. Ubuntu 24.04, kernel 6.8, Landlock ABI 4). The probe distinguishes `Supported` (ABI returned),
  `Disabled` (`EOPNOTSUPP`, built in but boot-disabled), and `Unsupported` (`ENOSYS`); the LSM-list
  membership is kept only as an extra observation, not as the ABI/net source of truth.

### Added
- Landlock-net preflight fields on the diagnostics report: `abi_probe_status` (`ok` / `unsupported` /
  `disabled` / `error`), `abi_probe_errno`, `abi_version_source`, `net_connect_tcp_supported` /
  `net_bind_tcp_supported` (ABI ≥ 4), and `no_new_privs_settable` (measured in a throwaway forked
  child, never set on the diagnostics process). Existing fields (`available`, `fs_enforce`,
  `net_enforce`, `abi_version`) are unchanged. This is preflight / host-eligibility only — it reports
  whether a host can support a future Landlock TCP-connect proof path; it does **not** implement or
  claim enforcement of TCP connects.
- Landlock-net CONNECT_TCP usability smoke on the diagnostics report: `net_connect_ruleset_probe`
  (`usable` / `unsupported` / `failed`) and `net_connect_ruleset_errno`. The smoke builds a
  CONNECT_TCP ruleset with a port rule (landlock crate, hard-requirement compatibility so the right is
  never silently best-effort-dropped) and applies it via `landlock_restrict_self` in a throwaway
  forked child that runs only async-signal-safe calls, so the diagnostics process itself is never
  restricted. This proves the host supports the CONNECT_TCP syscall path needed for a future
  enforcement proof; it does **not** implement or claim enforcement, and blocks no connection.

### Changed

- MCP execution-record verifiers: pin semantics with stable, machine-readable reason codes before the
  new verifiers are used as a contract (no new capability, no mode change, no schema bump).
  `verify-mcp-supersession` now exposes a stable `groups[].reason_code`
  (`supersession_resolved_*` / `supersession_ambiguous_*`) instead of only prose; the named fallback
  in `verify-mcp-records` distinguishes `fallback_projection_missing_authorization_binding` from
  `fallback_projection_invalid_meta` (both fail-closed). Tests now pin that the projection id is part
  of the digest preimage (changing it breaks the back-link) and that the whole `authorization_binding`
  object is bound (no allowlist inside the block). `--fallback-projection whole-envelope` is documented
  as the legacy compatibility mode and `named` as the named projection mode (default unchanged). The
  supersession report documents that `sequence` is asserted canonical-content ordering, not an
  independently verified ordering (Assay verifies no signatures). Docs:
  `docs/reference/cli/mcp-execution-record-fallback-plan.md`.

### Added

- `assay evidence verify-mcp-supersession`: independent-consumer evaluation of decision-record
  supersession for SEP-2828-style execution records. Given decision records that share a call binding
  (`backLink`), the latest `decidedAt` wins; an equal-`decidedAt` tie with no explicit ordering field
  (`decisionDerived.sequence`) is reported as `ambiguous` / non-conformant (exit `2`) rather than
  resolved from file order, arrival order, or the record nonce, because a nonce is unique per record,
  not an ordering field, and an arbitrary-but-deterministic winner can mask a producer that emitted two
  records that should never have tied. An explicit `sequence` resolves a tie deterministically.
  Consumer side only: no signature, issuer-trust, freshness, or runtime-truth claims.
- `assay evidence verify-mcp-records --fallback-projection named`: a no-attestation fallback binding
  computed over a named projection (the `tools/call` `params` plus the `_meta.authorization_binding`
  block) instead of the whole request envelope, so transport- or observation-local `_meta` fields a
  gateway/provider can legitimately add or strip do not change the binding digest. Allowlist (only the
  named fields are in the preimage) and fail-closed (a missing binding block is non-conformant, never a
  silent fall-back to hashing the whole envelope). The report carries a self-describing
  `binding.projection = "assay.fallback_projection.v0"`, so the rule is versioned and a change is an
  explicit bump; it tracks the in-progress SEP-2828 fallback-binding discussion. Default stays
  `whole-envelope`, so existing behavior is unchanged. Docs:
  `docs/reference/cli/mcp-execution-record-fallback-plan.md`.
- `assay project-otel` CLI: a read-only wrapper around the `otel::projection` library that emits
  `assay.otel_projection.v0` from files. `--capability-surface` is required; `--observation-health`
  and `--enforcement-health` are optional (following the library signature); `--out` writes to a file
  and leaves stdout empty. The CLI is transport only — it reads files, parses JSON, calls
  `assay_core::otel::projection::project`, and writes JSON; all projection semantics stay in the
  library. On a read/parse error it writes to stderr and exits `2` with empty stdout, without echoing
  raw artifact content. Not a telemetry pipeline: no OTLP export, no network, no runtime-proof claim.
  Docs: `docs/reference/otel-projection.md`.
- OTel GenAI + OpenInference projection (`otel::projection`, schema `assay.otel_projection.v0`): a
  read-only, one-directional, lossy view of assay runtime evidence (capability surface, observation
  health, enforcement health) as OpenTelemetry GenAI attributes plus an OpenInference `span.kind`, so
  an OTel/OpenInference backend can read assay evidence without learning assay's vocabulary. assay
  artifacts stay the source of truth; the output carries `lossy: true` and `source_of_truth` so the
  view cannot be mistaken for the record. Honesty invariants pinned by tests: every standard field that
  could be over-read carries a paired `assay.*` qualifier; enforcement is its OWN guardrail-style span
  (`assay.claim_class=enforcement`), never attributes hung next to an observed tool span, and absent
  when no `enforcement_health` is supplied (absence makes no claim); observed sets the standard
  vocabulary cannot express (egress endpoints, paths) stay under `assay.*`. Pinned to OTel GenAI semconv
  `1.37.0-development` (the agent/tool-span surface where `execute_tool` lives, distinct from the
  LLM-client-span surface the module pins at 1.28.0; both Development upstream) and OpenInference
  `pinned`, so a bump is explicit. Ships a
  contract doc (`docs/reference/otel-projection.md`) and a committed golden fixture (input plus expected
  projection) so an external reader sees the contract concretely. Projection function and fixtures only;
  no exporter and no CLI wiring (those are a later slice).

### Changed

- `policy_engine::PolicyState`: compile a policy's per-tool JSON Schema validators ONCE and reuse them
  across calls, instead of recompiling per call. The `args_valid` metric evaluator now compiles once
  per evaluation and reuses the validators across every tool call in the response (previously each call
  recompiled the matched tool's schema). `evaluate_tool_args` stays as the one-shot convenience and is
  unchanged; the MCP proxy already compiled at policy load. Verdicts are identical (parity-tested);
  this is a hot-loop performance change, not a behaviour change.

## [3.20.0] - 2026-06-09

### Added

- Enforcement health artifact (`assay.enforcement_health.v0`). `assay monitor --enforcement-health
  <path>` writes an explicit enforcement-truth artifact, deliberately SEPARATE from
  `observation_health`: observation_health answers "how complete was observation?", this answers "was
  enforcement active, and did it block?". The two are orthogonal (a run can have complete observation
  and absent enforcement, or vice versa), so they are not conflated into one blob. Fields:
  `network_enforcement` (`active` / `absent` / `failed` / `not_applicable`), `attach_confirmed`,
  `blocked_count`, `allowed_count`, `scope` (`ipv4_tcp_connect`). It is a written artifact, never parsed
  from stdout. Crucially, on the fail-closed abort path (egress enforcement requested but the connect4
  attach could not be installed) it writes `failed` BEFORE exiting, so a requested-but-failed
  enforcement is never mistaken for an un-requested one (`absent`). v0 is intentionally small; rule IDs,
  policy refs, timestamps, provenance, and enforcement receipts are follow-ups. The schema is
  producer-agnostic so future enforcement paths emit the same shape; a second enforcement domain becomes
  an explicit `v1`, never a silent reinterpretation of `v0`.

- Network egress enforcement (IPv4 TCP connect only). `assay monitor --policy <file>` now attaches the
  compiled `connect4` cgroup program so a policy's network deny rules actually block outbound connects,
  not just observe them. When the policy carries `net_connect` deny rules (a destination port or CIDR),
  `connect4_hook` is attached at the cgroup v2 root and the `DENY_PORTS` / `CIDR_RULES_V4` maps decide
  which connects are refused (EPERM); an empty rule set is a no-op. Previously the cgroup attach was a
  stub, so the compiled rules were never enforced at runtime.
  - **Fail-closed.** When enforcement is requested (the policy has network deny rules) but the attach
    cannot be installed (no cgroup v2 root, no kernel support, attach error), `assay monitor` aborts
    with exit code 4 (would-block) instead of degrading to audit-only. A caller asking for egress
    enforcement never gets a clean run that did not actually enforce.
  - **Bounded scope, explicit non-coverage.** This covers IPv4 TCP `connect()` egress only. It does NOT
    cover IPv6, UDP/QUIC, DNS resolution, already-open sockets, raw sockets, or proxy/tunnel identity.
    Policy semantics stay simple (a destination ip/port is allowed or denied); there is no
    provider classification or DNS-name truth here. The connect tracepoint observation path is unchanged,
    so `observation_health` reporting is unaffected by enforcement being active, and this change does not
    add a network-enforcement status to `observation_health` — consumers must not infer enforcement from
    observation coverage.

- URL userinfo redaction (ADR-034, Phase 3). A network endpoint that is a URL carrying a
  `user:pass@` credential pair now has its userinfo redacted at capture (`scheme://user:pass@host` ->
  `scheme://<redacted:url-userinfo:H8>@host`), preserving the scheme and host. It fires only when the
  userinfo contains a `:` pair (a token-as-username is already caught by the shape pass), is
  idempotent, and is a runner-side capture-hygiene transform rather than a shared detection rule.

- Secret-rule contract fixture (ADR-034, Phase 2). The runner Redactor's curated rules are now
  published as `secret-rules.v1.json` (the canonical name->pattern table), with a parity test
  asserting the built-in rules match it exactly; the same fixture is shared with the Plimsoll detector
  so the Rust and Python implementations cannot drift. Adds a `sensitive-query-param` rule covering
  URL/query credentials the assignment rule misses (`access_token=`, `sig=`, `signature=`).

- Runner evidence redaction at capture (ADR-034, Phase 1). The runner-spike run now redacts
  secret-shaped values (provider tokens, PEM keys, JWTs, bearer tokens, `key=value` credentials, and
  flag values such as `--token X`) out of argv and the capability surface before the bundle is
  serialized, hashed, or signed, replacing each with a value-free `<redacted:RULE:H8>` placeholder
  keyed by an installation-local key. `observation_health` gains an additive, value-free `redaction`
  block (mode, counts by rule and field, `key_scope`, `key_id`). A fail-closed assertion sweep aborts
  bundle creation if a secret-shaped value survives. Redaction is on by default; it can be disabled
  only with the deliberately named `--unsafe-disable-redaction` (recorded as `disabled_unsafe`). The
  redaction key resolves from `ASSAY_REDACTION_KEY_FILE`, else a generated host-local key file, else
  `--redaction-key ephemeral`. Note: default-on redaction changes the recorded bytes of bundles that
  contained secret-shaped values; clean bundles are byte-identical and all existing bundles remain
  valid. This is a runner behavior change and should ship with a minor version bump.

## [3.19.1] - 2026-06-07

### Fixed

- Validate gzip trailer CRC/ISIZE during evidence bundle verification so
  truncated or corrupted gzip payloads fail before content validation. (#1559)
- Keep runner Assay binaries fresh against the latest release, preserve the
  GitHub Action v3.0 sandbox and attestation contract, and add a release-line
  version gate covering workspace, Harness, and VM surfaces. (#1558)

## [3.19.0] - 2026-06-06

### Added

- `assay evidence attest` — sign an evidence bundle's manifest as an in-toto v1
  Statement, emitted as a DSSE envelope (Ed25519 over the JCS-canonicalized
  statement), using a PKCS#8 PEM key from `assay mcp tool keygen`. Builds on the
  ADR-039 attestation library (shipped library-only in 3.18.0). The anchor
  (transparency log / timestamp) stays external; an attestation binds who-said-it
  and the bundle content and does not upgrade observed support. Predicate type is
  a non-committal `v0`.

## [3.18.0] - 2026-06-06

- Added OTel GenAI `execute_tool` emission helpers in `assay-core` and
  `assay sandbox --otel-jsonl`. The emitted records carry bounded
  claim-class outcome fields for sandbox observations and keep OTel as an
  export/interchange surface, not the authoritative evidence or policy truth
  layer.

- Added in-toto/DSSE attestation support over evidence bundle manifests in
  `assay-evidence`. The attestation helper signs the bundle manifest digest and
  records the envelope material needed by downstream verifiers without
  promoting issuer trust, application outcome truth, or bundle-content
  correctness beyond the signed manifest boundary.

- Added the `assay-it` Python claim-support scorer for Inspect-oriented
  consumers. The helper aggregates observed claim support into bounded
  categories that downstream harnesses can consume, while leaving policy
  enforcement, signer trust, and application correctness decisions outside the
  scorer.

- Updated coding-agent governance docs and README discoverability for the
  sandbox evidence bundle, OTel JSONL export, bundle attestation, and Inspect
  claim-support scorer. These notes describe the technical contract seam and
  artifact flow for downstream consumers; they do not add a broader runtime
  safety, sandbox correctness, or governance-status claim.

- Fixed the release workflow's cross-target binary build setup by installing
  each matrix Rust target explicitly before building. This keeps the release
  artifact path aligned across the CLI, MCP server, wheels, proof kit, and the
  existing crates.io publish order.

## [3.17.0] - 2026-06-06

- Added `assay sandbox --bundle-out`, which emits sandbox observations as a
  canonical evidence bundle. The bundle projection records observed filesystem,
  environment, process, and sandbox-degradation facts without promoting them to
  policy approval, signer trust, application outcome truth, or a broader
  runtime-safety claim.

- Hardened Runner/eBPF release and CI behavior by documenting unsafe invariants
  in eBPF and runner Linux code, adding targeted unsafe lint posture, pinning
  the eBPF toolchain, and making native eBPF builds use the release-optimized
  path expected by the kernel verifier. These changes keep the runner proof
  path bounded and do not change the public evidence archive schema.

- Split high-traffic implementation hotspots behind stable facades, including
  CLI importers and command modules, registry trust/cache/resolver internals,
  runner path projection, policy tier compilation, metrics argument validation,
  simulation attack matrices, and mandate core data types. Public module
  surfaces are preserved through re-exports; the release adds contract and
  serialization guards where the moved code carries evidence or policy
  semantics.

- Added and refined technical governance docs for the Assay/Runner/Harness
  contract seam, sandbox-evidence capture, editor MCP wrapping, OTLP export for
  observations, evidence-bundle attestation, Inspect claim-support scoring, and
  the eBPF policy-substrate decision boundary. These are repository contracts
  and implementation guidance, not separate product claims.

- Replaced historical per-wave refactor artifacts with the durable generic
  split-wave gate and removed stale split review scripts. Routine refactor
  waves now keep move maps and review notes in PR bodies plus the rolling
  refactor status page instead of adding per-wave `SPLIT-*` docs or
  `review-wave*.sh` scripts.

- Documentation: grouped the coverage-honesty examples under a single
  "Coverage honesty" section in the examples index, with the end-to-end
  walkthrough as the entry point, so the capture → coverage descriptor →
  annotation → enforcement → aggregation chain is discoverable in one place.
  Also added Runner reference docs for the address-less and non-IP
  `sendto`/`sendmsg` send counters. Documentation only; no schema, archive,
  CLI output, or other contract change.

## [3.16.0] - 2026-06-04

- Added `assay evidence verify-mcp-tunnel-observed`, a bounded consumer-side
  checker for MCP tunnel observed-facts fixtures. The command validates the
  `assay.mcp.tunnel_observed.v0` shape, enforces no-raw-payload and
  no-raw-authorization boundaries, and classifies whether evidence references
  support a strong `same_request_instance` join or only diagnostic correlation.
  It does not prove tunnel mediation, authorization success, policy
  correctness, tool result truth, application outcome truth, or issuer/key
  trust.

- Added Runner coverage descriptor helpers and examples for coverage-aware
  side-effect interpretation. The new `assay.runner.coverage_descriptor.v0`
  helper gates positive, exhaustive, and bounded-negative effect claims by
  effect dimension and documented blind spots, so observed positives can remain
  useful while absence and exact-set claims stay blocked or degraded when the
  capture method cannot support them.

- Added coverage-aware drift annotation and enforcement support to the
  cross-runtime drift experiment comparator. The comparator can now emit a
  sidecar claim annotation, derive measured-positive strength from per-arm
  observation health, and use `--assert-claim TYPE:DIMENSION` to fail when a
  requested claim is not permitted by the coverage/fidelity gates. The drift
  report schema remains unchanged.

- Added datagram-aware network coverage descriptors for Runner archives that
  report `datagram_peer_observed` or `connect_and_datagram_peer_observed`.
  Coverage-aware samples now derive the network descriptor from
  `observation_health.network_protocol_coverage` instead of assuming
  `connect_only`. Datagram peer evidence strengthens positive network
  observations, but exact peer-set and bounded-negative network claims remain
  degraded or blocked while blind spots are declared.

- Updated dependency and CI hygiene patches, including CodeQL, `serial_test`,
  `uuid`, and `assert_fs` patch bumps.

## [3.15.0] - 2026-06-03

- Added Runner network-fidelity claim-scope fields so measured-run archives can
  distinguish capture health from protocol coverage. `network_protocol_coverage`
  now records whether evidence is connect-only, datagram-peer-only, or both, and
  `network_endpoint_claim_scope` keeps raw network endpoints diagnostic-only
  when Assay cannot make an exact peer-set claim.

- Added Runner datagram peer telemetry for Linux captures by attaching
  `sys_enter_sendto` and `sys_enter_sendmsg` tracepoints alongside the existing
  `connect()` hook. Assay now records observed datagram destination sockaddr
  evidence when the kernel exposes it, while still avoiding request-level,
  `cf_ray`, or authoritative exact-QUIC-peer binding claims.

- Added Runner fidelity helpers for low-level archive consumers, including the
  fidelity verdict helper and declared path-projection helper used by measured
  run proof bundles and runner documentation.

- Improved MCP execution-record verification by adding outcome decision-digest
  verification and a request-envelope fallback binding path for supported MCP
  execution-record pairing fixtures.

- Updated the cross-runtime drift experiment comparator so raw
  `network_endpoints` churn is classified as inconclusive when either archive
  declares diagnostic-only network endpoint scope. This prevents experiment
  tooling from turning deliberately weak Runner transport evidence into a hard
  provider/runtime drift claim.

- Added interop joinability summaries for the agent-observability fidelity
  experiment docs, keeping those artifacts experiment-scoped and non-product
  API.

## [3.14.0] - 2026-06-01

- Added `assay evidence verify-mcp-records`, a downstream consumer verifier
  for SEP-2787 attestation and server execution-record fixture pairing. The
  command computes the SEP-2787 JCS digest, checks decision/outcome `backLink`
  fields, and emits an `assay.mcp.execution-record-pairing.report.v0` report.
  It does not verify signatures, establish issuer key trust, proxy MCP, prove
  policy correctness, prove side effects, or claim runtime truth.

- Grouped policy authoring under `assay policy generate` and
  `assay policy record`. The previous top-level `assay generate` and
  `assay record` commands remain available as hidden compatibility shims with
  stderr deprecation warnings; output shapes, exit codes, and generated policy
  behavior are unchanged.

## [3.13.0] - 2026-06-01

> `v3.13.0` closes the post-`v3.12.0` CLI UX pass and ships the first
> selective command-grouping pilot. It keeps the core evaluation loop flat,
> adds machine-readable `run` output, improves trace/validate ergonomics,
> canonicalizes Trust Card spelling, and groups MCP runtime commands under
> `assay mcp` while preserving hidden compatibility shims for the previous
> flat paths.

- Grouped MCP runtime commands under the visible `assay mcp` command
  family. The canonical forms are now `assay mcp discover`, `assay mcp
  kill`, and `assay mcp tool ...`; the previous flat `assay discover`,
  `assay kill`, and `assay tool ...` paths remain available as hidden
  compatibility shims with stderr deprecation warnings. Output shapes,
  exit codes, artifacts, and MCP policy behavior are unchanged.

- Added `assay run --format <text|json>`. `text` (default) keeps the
  existing human-readable summary on stderr; `json` prints a
  machine-readable results report to stdout so `assay run --format json >
  results.json` composes with CI pipelines. The `run.json`/`summary.json`
  artifacts and the exit-code contract are unchanged. This mirrors the
  existing `assay validate --format` interface for consistency.

- Tightened trace-replay UX: `model: trace` now fails early with
  `E_INVALID_ARGS` when `--trace-file` is missing instead of falling
  through to misleading test failures.

- Added the natural positional config form for validation:
  `assay validate eval.yaml --trace-file traces.jsonl`. The existing
  `--config eval.yaml` form remains supported.

- Renamed the Trust Card command surface to `assay trust-card` for
  consistency with other hyphenated multi-word commands. The previous
  `assay trustcard` spelling remains available as a deprecated
  compatibility alias.

- Surfaced the synthetic MCP tool evidence-binding quickstart from the
  observability reference, the research note, and the root README
  research section. Discoverability only: no new schema, no top-level
  example, no poisoning-detection or product-API claim.
- Added a boundary-first quickstart for the synthetic MCP tool
  evidence-binding harness. It demonstrates bounded
  `description -> call -> effect -> claim` reading without promoting the
  schema, contacting live MCP servers, or claiming poisoning detection.
- Added checked-in starter outputs for the synthetic MCP tool
  evidence-binding harness and a regression test that regenerates them
  to catch harness/output drift.
- Clarified that the MCP tool evidence-binding harness's synthetic
  tunnel/proxy transport fixture is context-only metadata and does not
  prove tool intent, upstream MCP authentication, poisoning, or stronger
  description/call/effect claims.
- Added a synthetic MCP tool evidence-binding harness that emits
  experiment-scoped `binding_cell.v0` rows for description/call/effect
  scenarios, including plural visible tool-description sets and one
  tunnel-context fixture. The harness does not contact live MCP servers,
  detect poisoned tools, classify maliciousness, rank MCP
  implementations, or promote a receipt family.
- Smoke-verified the first post-closure delegated semantic-gap sidecar:
  run `26620643517` passed the `openai-agents-hidden-write` delegated
  gate and same-head positive baseline, recording a bounded
  `hidden_write` `semantic_gap` row without publishing other delegated
  gap scenarios, classifying maliciousness, or promoting experiment
  artifacts to product APIs.
- Hardened the delegated `hidden_write` smoke record by normalizing
  workdir-containment checks instead of relying on string-prefix
  matching, and clarified the time-limited proof-pack artifact versus
  durable run/SHA/hash provenance.
- Hardened the local `assay-bpf-runner` health check so unattended
  cache-count probes do not emit invalid numeric comparisons when the
  remote `find` path is empty or unavailable.
- Added an opt-in delegated `hidden_write` semantic-gap expansion gate under
  the `Runner Spike Delegated` workflow's `gates=all` path. The existing
  `openai-agents-kernel-policy` baseline gate remains unchanged; the new
  wrapper reuses the OpenAI Agents fixture with an explicit scenario selector
  and asserts a workdir-bounded write/create effect without upgrading it to
  maliciousness, policy-failure, or root-cause evidence.
- Added a post-closure delegated semantic-gap expansion plan for
  `hidden_write` after the smoke-verified `matched_safe_read` baseline.
  The plan pins the technical review gate without dispatching a run,
  publishing a gap finding, defining schemas, or promoting artifacts.
- Clarified the experiment arc lifecycle rules for post-closure
  follow-up plans: they must keep findings summaries closed, land any
  new finding as a sidecar, and avoid hidden arc reopening.
- Added an observability fidelity calibration reference note that generalizes
  the closed overhead and fidelity arcs' requested-vs-observed
  calibration lesson. The note frames retained signal as a prerequisite
  for timing, throughput, and absence claims without opening a new
  experiment arc, defining a schema, or promoting experiment-scoped
  calibration artifacts to product APIs.
- Added a research note for MCP tool evidence binding. The note asks
  what bounded evidence is needed to connect the model-visible MCP tool
  context, a tool call, and a measured runtime effect. It does not
  attempt tool-poisoning detection, create a receipt family, define a
  schema, rank MCP implementations, open a new experiment arc, or publish
  outreach targets.
- Removed the legacy `assay-runner-spike` compatibility wrapper crate
  from the workspace and release pipeline. The runner substrate publish
  contract now includes only `assay-runner-schema`,
  `assay-runner-core`, and `assay-runner-linux`; release/public-crate
  policy scripts and runner boundary docs were updated accordingly.
- Added a post-arc claim-boundary positioning note for agent
  observability work. The note records Assay's post-overhead and
  post-fidelity position as a claim-boundary and evidence-fidelity layer,
  not an observability replacement. It records public next-arc selection
  discipline without publishing outreach targets, comment drafts,
  adjacent-whitespace shortlists, competitive analysis, or private
  sequencing notes.
- Added an experiment arc lifecycle guide that captures the shared
  plan-to-harness-to-findings-summary pattern proven by the overhead and
  agent-observability fidelity arcs. The guide documents delegated gate
  discipline, the separate research-evidence versus engineering
  compliance proof tracks, closure rules, and promotion non-triggers
  without opening a new arc or promoting experiment-scoped schemas.
- Closed the agent-observability fidelity arc with a citation-oriented
  findings summary. The summary bounds five claims: calibration as a
  mechanical guardrail, evidence packs as non-strengthening carriers,
  the six-scenario synthetic semantic-gap matrix, the five-cell interop
  coverage matrix, and the delegated `matched_safe_read` positive
  baseline smoke. It does not publish delegated gap findings, rank trace
  vocabularies or products, or promote experiment-scoped schemas to
  product APIs.
- Smoke-verified Slice 7 of the agent-observability fidelity roadmap
  with a delegated `matched_safe_read` baseline. GitHub Actions run
  `26571739019` passed the existing `openai-agents-kernel-policy`
  delegated Runner gate, uploaded proof pack
  `assay-runner-delegated-proof-pack-26571739019`, and records clean
  Runner health, a strong `tool_call_id=tc_runner_policy_001` join, and
  a `positive_join` scenario verdict without publishing delegated gap
  scenarios or promoting experiment artifacts to product APIs. The slice
  also hardens Linux cgroup root selection so Assay skips systemd
  `.service` units as session roots, matching the existing `.scope`
  handling.
- Planned Slice 7 of the agent-observability fidelity roadmap as a
  delegated semantic-gap baseline. The plan pins the existing
  `openai-agents-kernel-policy` delegated Runner gate, required
  proof-pack artifacts, clean-health and strong `tool_call_id` join
  invariants, and non-claims before any semantic-gap finding is promoted
  beyond synthetic harness behavior. The roadmap now reserves the next
  closure step for a fidelity-arc findings summary and keeps the OTel
  span-limit study trigger-only.
- Added the Slice 6 synthetic interop harness for the
  agent-observability fidelity roadmap. The harness emits five
  OTel GenAI / OpenInference / Runner starter cells with strict
  `assay.experiment.agent_observability_fidelity.interop_coverage_cell.v0`
  rows, source snapshots, join-result references, claim-class
  references, and partial/absent coverage rows without delegated
  measurements, product ranking, runtime translation, or product API
  promotion.
- Planned Slice 5 of the agent-observability fidelity roadmap as an
  OTel GenAI / OpenInference / Runner interop matrix. The plan pins
  coverage axes, upstream snapshot rules, five starter cells, the
  future `interop_coverage_cell.v0` row shape, and non-claims so the
  matrix remains a coverage and claim-strength map rather than a
  product ranking or runtime translator. Interop mapping now moves from
  `proposed` to `experiment-scoped` in the artifact-families inventory
  without creating a release-facing product API.
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
- Expanded the semantic-gap synthetic harness to all six predeclared
  scenario-plan rows: `matched_safe_read`, `path_rewrite`,
  `hidden_write`, `retry_self_correction`, `runtime_side_effect`, and
  `weak_join_fallback`. The harness still does not dispatch delegated
  runs or publish semantic-gap findings.

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
