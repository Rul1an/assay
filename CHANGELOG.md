# Changelog

All notable changes to this project will be documented in this file.

## [Unreleased]

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
