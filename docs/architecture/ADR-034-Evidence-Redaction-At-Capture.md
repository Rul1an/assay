# ADR-034: Evidence Redaction at Capture (runner-side secret hygiene)

## Status

Proposed (June 2026). DRAFT, reviewer feedback incorporated; design decisions resolved (see Decisions).
Not yet implemented; awaiting go on Phase 1.

Companion to the already-shipped consumer-side detector (Plimsoll `PLIMSOLL-POSSIBLE-SECRET`,
plimsoll #11 / action engine 0.3.0). That check detects a secret-shaped value *after* it has already
been written to an evidence record and pushes the fix back here. This ADR specifies the *here*: keeping
the secret out of the artifact in the first place.

## Context

The assay runner records a capability surface and a set of run events. Several recorded fields carry
values that originate from the runtime and can embed credentials:

- `command` (the full argv) is serialized into the `run_started` event
  (`assay-runner-core/src/run.rs`, `append_run_started`). This is the highest-risk vector: agent
  launch lines routinely contain `--token X`, `--api-key=sk-...`, `--password=...`.
- `capability_surface.filesystem_paths`, `network_endpoints`, `process_execs` are populated from kernel
  monitor events (`assay-runner-core/src/kernel.rs`, `push_monitor_event`). A path or a URL can embed a
  presigned token or a query-string credential; an exec value can be a script path with an embedded
  secret.
- `capability_surface.mcp_tools` and `policy_decisions` are tool names from `assay.tool.decision` events
  (`assay-runner-core/src/policy.rs`). Low risk, but not zero (a tool argument echoed into a name).
- `env` is already recorded keys-only (`run_started` emits `env_keys`, never values). Good. This ADR
  turns that good behavior into an enforced invariant rather than an incidental one.

Two facts make this a real defect, not a nicety:

1. An evidence bundle is content-addressed and often retained, shared, and uploaded to code scanning. A
   secret inside it is a durable leak in an artifact whose whole purpose is to be kept and inspected.
2. The same value tends to be high-churn, so a credential embedded in a surface also pollutes
   release-over-release diffs with noise.

This was surfaced concretely by appium-mcp #386, where a raw-capabilities dump into an evidence record
was narrowed to an allowlisted projection precisely because raw caps carry credentials, device ids, and
vendor options.

## Design principle: evidence minimization

This ADR is, at heart, not about secret scanning. Secret redaction is one instance of a more general
rule that already runs through the Runner and Plimsoll line:

> **Evidence minimization.** Record the minimum information needed to answer the review question. If a
> review question can be answered from a projection, the projection is preferred over the raw value.

The whole codebase already follows this:

- `env_keys`, never env values.
- `host:port`, not the full URL.
- a tool name, not the full tool payload.
- `kernel_layer` / `network_protocol_coverage` / `cgroup_correlation`: a coverage descriptor, not a
  raw event firehose.
- `inconclusive_observation_gap` and the fail-closed review: say what was and was not observed, do not
  fabricate a clean pass.

Secret redaction extends the same principle: a redacted token *class* (`<redacted:github-token:H8>`)
answers the only review question that matters here ("a github-token-shaped value passed through this
field, and it is the same one that appeared elsewhere") without recording the token itself. Where this
ADR and the general principle disagree, the principle wins: prefer not recording a value over recording
and redacting it.

## SOTA grounding (June 2026)

The design follows the current consensus for secret hygiene and evidence sanitization:

- Curated provider rulesets over generic entropy (gitleaks / trufflehog lineage): match known token
  shapes and structural `key=value` credentials; do not flag high-entropy strings wholesale, because
  content-addressed digests and hashes are legitimately high-entropy and would be false positives.
- Redact at the boundary, structurally, the way OTel collector redaction / attribute processors strip
  sensitive attributes before export rather than at the sink.
- Allowlist projection over denylist scrubbing wherever the field has a knowable safe shape (argv[0] is
  a binary path; an endpoint is host:port). Denylist scanning is the fallback for free-form fields.
- Data minimization as a first principle: record the narrowest projection that still answers "what
  capability changed", never a raw config or environment dump.
- Defense in depth: a value-level redaction pass at each `add_*` boundary, plus a final sweep over the
  serialized bytes before hashing, so a missed funnel still cannot ship a raw secret.
- Determinism is non-negotiable here: assay evidence is replayable (VCR) and Merkle-hashed. Redaction
  must be a pure, deterministic transform applied *before* hashing, so the bundle hash covers the
  redacted form and the raw secret never enters the hash input (you cannot brute-force a secret back out
  of the manifest).

## Goals

- No raw secret-shaped value in any serialized byte of a default-mode bundle (argv, surface fields,
  event ndjson).
- Redaction is deterministic and replay-stable.
- Redaction is honest: the bundle states that redaction happened, how many values, of which rule class,
  in which field, without echoing the value.
- Single source of truth for the rule set, shared with the Plimsoll detector, with a parity test.
- Default-on, with an explicit, logged escape hatch for trusted local debugging.

## Non-goals

- This is not a DLP product, not encryption, and not a guarantee of secret-freedom. It is a best-effort,
  shape-based hygiene pass. The evidence and docs must say "best-effort redaction", never "guaranteed
  secret-free".
- Not a policy/enforcement decision: redaction never changes a gate verdict or coverage. It changes the
  recorded value, not whether the event was observed.

## Design

### 1. A `Redactor` with one rule set

Introduce a small redaction module (proposed `assay-runner-core::redact`, userspace only, not eBPF).
It holds a compiled rule set: the same curated provider/structural shapes as the Plimsoll detector
(AWS / GitHub / OpenAI / Slack / Stripe / Google tokens, PEM private keys, JWTs, bearer tokens, and a
generic `key=value` credential-assignment rule), plus a flag-aware rule for argv (see 3). No generic
entropy rule.

`Redactor::redact_str(&self, field: Field, input: &str) -> (Cow<str>, SmallVec<Hit>)` returns the
redacted string and the value-free hits (field, rule, count). It is pure given the rule set and the
run-scoped salt.

### 2. Placeholder format (deterministic, shape-preserving, non-reversible)

A matched span is replaced by:

```
<redacted:RULE:H8>
```

where `RULE` is the rule name (e.g. `github-token`) and `H8` is the first 8 hex chars of
`HMAC-SHA256(key = installation_secret, msg = matched_value)`.

The key is an installation/org-scoped secret (`installation_secret`), NOT `run_id`. This is the
decided choice (see Decisions). Properties:

- Stable within an installation/org: the same secret redacts to the same placeholder across runs and
  releases, so "this same credential was reused / reappeared" stays visible to a security reviewer
  across a release-over-release diff, which is exactly the kind of signal a reviewer wants. A per-run
  salt would have destroyed that.
- Deterministic and replay-stable: same `(installation_secret, value)` always yields the same token, so
  VCR replay and Merkle hashing stay stable.
- Not reversible: the raw value is keyed-hashed, never stored, never logged. The keyed hash also means a
  bundle leaked without the installation secret cannot be brute-forced back to the value by an outsider
  (an unkeyed hash of a short/low-entropy secret would be brute-forceable).
- We do NOT record `matched_len` in the runner evidence (unlike the Plimsoll detector, which sees the
  value at review time): length leaks bits about the secret. The runner records rule + count only.

Key management (the one new operational surface this introduces): `installation_secret` is a
locally-held value, resolved from config or an env var, generated once per install if absent and
persisted alongside the runner config. It is a redaction salt, not an encryption key: losing it only
means future redaction tokens stop correlating with older bundles; it never exposes a past secret. It
must never itself be written into any evidence field (add it to the env keys-only / never-recorded
set).

### 3. Field-by-field treatment

- `command` / argv (highest risk). Two passes: (a) flag-aware: for a known credential flag
  (`--token`, `--api-key`, `--password`, `--secret`, `-p`, and `KEY=VALUE` forms), redact the value
  regardless of shape, because a short password is not shape-matchable; (b) shape pass over each
  remaining token. `argv[0]` (the binary path) is treated as a path, not a credential.
- `filesystem_paths`: shape pass on the whole path string.
- `process_execs`: shape pass PLUS the same flag-aware pass as argv. The boundary between `command`
  (the launch line we record in the run event) and `process_exec` (an exec observed by the kernel) is
  not clean: an observed exec can itself be a full invocation like `python script.py --token ...` or a
  script path with a query string `/tmp/run.sh?token=...`. So `process_execs` gets the argv treatment,
  not just a path shape pass.
- `network_endpoints`: if the value parses as a URL, redact `userinfo`, known sensitive query params
  (`token`, `key`, `sig`, `signature`, `access_token`, ...), AND the URL `fragment` (e.g.
  `#token=...`, which carries credentials just as query strings do), structurally, then a shape pass on
  the remainder. If it is bare `host:port` (the common kernel-connect case), it is left as is.
- `mcp_tools`, `policy_decisions`: shape pass only.
- `env`: codify keys-only as an invariant. Add a test asserting no env *value* ever reaches any
  serialized event. A `--capture-env-values` escape hatch, if ever added, must redact through the same
  Redactor and is out of scope here.

### 4. Where it runs (two chokepoints)

- Primary: at the value boundary. The capture structs (`KernelLayerCapture`, `PolicyLayerCapture`, the
  run-event builder) take a `&Redactor` and pass every string-valued field through it before it is
  inserted into `CapabilitySurface` / written into an event. The raw value never lives in the in-memory
  surface.
- Belt-and-suspenders: a final ASSERTION sweep over the assembled ndjson before the Merkle root and
  manifest are computed. This sweep does NOT rewrite bytes (rewriting serialized evidence is hard to
  reason about and muddies the semantics of "what was captured"). Instead it fails closed: if the shape
  rules still match anything after the boundary pass, bundle creation aborts with an error rather than
  producing a bundle. A match here means a capture funnel was missed, which is a runner bug to fix, not
  a value to silently rewrite. The primary defense is the capture-boundary redaction; this sweep is a
  fail-closed backstop, not a second redactor. It applies the same rule set AND the same allowlist as
  the boundary pass, so an allowlisted-safe value does not trip a spurious failure, and an already
  redacted `<redacted:...>` placeholder no longer matches any rule.

Redaction happens strictly before hashing/signing, so the manifest and signature cover the redacted
content and the raw value is absent from the hash preimage. The assertion sweep likewise runs before
hashing, so a missed funnel can never reach a signed/stored artifact.

### 5. Honesty: an `observation_health.redaction` block

Add an additive field to `assay.runner.observation_health.v0` (or a `.v1` bump if the field set is
frozen):

```json
"redaction": {
  "mode": "shape_and_flag",
  "redacted_count": 3,
  "by_rule": { "github-token": 2, "credential-assignment": 1 },
  "by_field": { "command": 2, "filesystem_paths": 1 }
}
```

This makes the evidence state plainly that redaction occurred and of what class, with no value echoed.
The Plimsoll consumer can then soften `PLIMSOLL-POSSIBLE-SECRET` from "a secret is sitting in your
evidence" to "N values were redacted at capture (rule X)", which is the honest end state: capture-side
prevention with a consumer-side receipt.

Coverage is unaffected by design: redaction changes the recorded value, not whether the layer was
observed. `policy_layer` / `kernel_layer` / `network_protocol_coverage` keep their current meaning.

### 6. Configuration

- `--redact <shape_and_flag|shape_only>`; default `shape_and_flag`. There is no `off` value on this
  flag, deliberately.
- `--redact-allowlist <file>`: regexes for known-safe values to suppress false positives.
- The ONLY way to disable redaction is a separate, deliberately alarming flag:
  `--unsafe-disable-redaction`. Choosing a scary name over a neutral `--redact off` is intentional:
  users override defaults, and the flag name itself must say "this is dangerous". When set, the runner
  (a) prints a prominent stderr warning that the resulting bundle may contain raw credentials and must
  not be shared or retained, and (b) records `observation_health.redaction.mode = "disabled_unsafe"` so
  any downstream reviewer can see the bundle was never sanitized. There is no silent way to disable it.

## Rule-set parity with Plimsoll (single source of truth)

The runner (Rust) and the Plimsoll detector (Python) must agree on what a secret looks like. Proposal:
a versioned, language-neutral fixture `secret-rules.v1.json` (rule name plus a set of
`{input, expected_rule, expected_redacted}` vectors), committed to both repos, with a parity test in
each that asserts its implementation matches every vector. This avoids a cross-language runtime
dependency while preventing drift. The rule *patterns* themselves stay implemented natively in each
language (Rust `regex`, Python `re`); the fixture is the contract.

## Backwards compatibility and migration

- The `observation_health.redaction` field is additive; old consumers ignore it.
- Default-on redaction is a behavior change to recorded values, so it ships behind a minor version bump
  with a CHANGELOG entry and a known-issue note, and `--redact off` reproduces pre-redaction byte output
  for anyone diffing against historical bundles.
- Existing bundles are untouched.

## Performance

The hot path is `push_monitor_event`. Mitigations: a single precompiled `RegexSet` for the cheap
"does anything match at all" check before any per-rule work; scan only string-valued fields; cap the
scanned length per value; avoid allocation when there is no hit (`Cow::Borrowed`). Add a criterion bench
(`redact/clean_path`, `redact/argv_with_token`) and hold it to the existing tail-ratio budgets in
`docs/PERFORMANCE-ASSESSMENT.md`.

## Security considerations

- Best-effort only; never claim guaranteed secret-free.
- The raw value is never logged, never stored, and never used as a hash preimage that ships; only the
  keyed-hash prefix ships.
- Placeholder tokens are intentionally shaped so they cannot be mistaken for a real credential by a
  downstream scanner (the `<redacted:...>` wrapper).
- A secret split across two fields can defeat shape matching; this is a known limitation, documented.
- Length is not recorded by the runner, to avoid leaking entropy about the secret.

## Testing

- Per-rule unit tests (match, redact, no-echo).
- Determinism: same `(run_id, value)` redacts identically; idempotent (redacting twice is a no-op on the
  placeholder).
- Invariant test: assemble a full archive containing planted secret-shaped values in argv, a path, and a
  URL query string, then assert no raw planted value appears in any serialized byte of the bundle.
- env invariant: no env value ever reaches a serialized event.
- Parity test against `secret-rules.v1.json`, mirrored in Plimsoll.
- Criterion bench on the hot path.
- Test fixtures assemble planted secrets from fragments at runtime (so the repo secret scanner does not
  flag the test files, the same pattern used in the Plimsoll detector tests).

## Rollout plan

1. Phase 1: `Redactor`, shape pass on argv and capability-surface fields, the `observation_health.
   redaction` block, the fail-closed assertion sweep before hashing, env keys-only invariant test,
   `--unsafe-disable-redaction` escape hatch, default-on, and the installation-secret salt plumbing.
   Minor version bump.
2. Phase 2: flag-aware argv value redaction (catches non-shaped secrets like short passwords).
3. Phase 3: URL userinfo and sensitive query-param redaction for `network_endpoints`.
4. Phase 4: shared `secret-rules.v1.json` fixture and parity tests in both repos; Plimsoll consumes
   `observation_health.redaction` to downgrade `PLIMSOLL-POSSIBLE-SECRET` to a "redacted at capture"
   informational note when the runner already handled it.

## Decisions (resolved in review, June 2026)

1. **Salt: installation/org secret, not `run_id`.** A per-run salt would hide secret reuse across runs
   and releases; an installation-scoped key keeps "same secret reappeared" visible to a reviewer while
   staying non-reversible. (See "Placeholder format" and its key-management note.)
2. **Default mode: default-on (`shape_and_flag`) from Phase 1.** The risk of an opt-out window (raw
   secrets shipping in the interim) outweighs the byte-output compatibility cost. Disabling is only via
   `--unsafe-disable-redaction`.
3. **Schema: additive `redaction` field on `observation_health.v0`.** This is metadata; no `.v1` bump.
4. **Length: record nothing.** No `short|medium|long` bucket. No real triage value, and a potential
   leak of entropy about the secret.
5. **Rule-set home: a `secret-rules.v1.json` contract fixture in both repos.** No generated runtime
   shared source (too much machinery). Same approach as the claim-class fixtures.

### Residual operational detail to confirm during Phase 1

The installation-secret salt is the one new surface this introduces. Phase 1 must decide where it is
stored and how it is generated/rotated (proposal: generated once per install, persisted with runner
config, resolvable via config or env, never written into evidence). This is an implementation detail,
not a blocker for the design.
