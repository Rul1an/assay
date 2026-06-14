# C2 — privileged-action PR-gate demo

A runnable, offline example that shows the enforcing proxy decide a privileged tool action before it
forwards, with the per-call evidence record, and a CI PR-gate built on it. The example is the new
`See It Work` entry point; the existing filesystem `wrap` walkthrough stays as a secondary example.

## Goal

One known-bad privileged action, denied three ways before forward, then an allowed path once it is
declared, scoped, and re-approved. Everything offline against a local mock MCP server: no real
credentials, no real provider, no side effect. The point is the decision and the evidence, not a
live exploit.

## Scenario

An agent reaches a GitHub MCP server through the enforcing proxy and tries `github.add_deploy_key`
on `acme/prod-app` — a privileged in-application write. The proxy decides per call and emits the
canonical `assay.enforcement_decision.v0` record. The exact emitted reason codes (verified against
`crates/assay-mcp-server/src/proxy/enforce.rs`):

| Axis (gate) | Deny reason code | Why |
|---|---|---|
| caller-allowance (c2) | `no_declared_allowance` | the action class is not in the approved allowances |
| credential-scope (c3a) | `credential_scope_insufficient` | the declared credential's scopes do not cover the action |
| manifest-drift (c3b) | `manifest_drifted_since_approval` | the observed per-tool digest differs from the approved baseline (a post-approval change) |

The allowed path: declare the action, scope the credential, re-approve the surface → `allow` →
forwarded to the **local mock** (never a real provider).

## Conformance evidence is separate, not a deny

Alongside the verdict the proxy can emit `assay.tool_annotation_conformance.v0`. When the server
declares a tool read-only but the observed call is mutating, that record carries
`conformance: "mismatched"` with `mismatch_kind: "declared_read_only_observed_mutating"`.

This is **separate evidence beside the verdict, recorded but non-gating** — Increment 5 made the
conformance carrier orthogonal on purpose. The demo frames it as "declared read-only, observed
mutating," never as "denied because of an annotation mismatch." A gating policy on conformance would
be a new, explicit slice, out of scope here.

## Artifacts the example produces

- the terminal deny/allow output with the exact reason codes;
- one `assay.enforcement_decision.v0` record per call (replayable);
- one `assay.tool_annotation_conformance.v0` record for the read-only-vs-mutating case (separate);
- a contract-fixture corpus of the scenario (inputs → expected verdicts + reason codes) for other
  implementations to reproduce.

## Bounded non-claims (shown in the example)

- a deny is fail-closed caution, not a maliciousness verdict;
- an allow is the decision to forward, never proof the action happened (allow is not delivery);
- the example runs against a local mock — no real provider, no real side effect, no exploit claim;
- the conformance mismatch is separate evidence; it does not change the verdict or gate the call;
- observed behavior is the proxy's classification of the call, not verification of the upstream side
  effect.

## Slices

- **C2-1** — `examples/privileged-action-gate/`: a local mock MCP server exposing
  `github.add_deploy_key`, a policy, an approved baseline, a one-command runner, and the expected
  output. Offline and deterministic. No runtime change.
- **C2-2** — wire the three deny axes and the allowed path as scenario configuration over the
  existing gates (no PDP change), surfacing the exact reason codes and emitting the
  `enforcement_decision.v0` records plus the separate `tool_annotation_conformance.v0` record.
- **C2-3** — a checked-in VHS tape; a CI job regenerates and smoke-tests the render (the tape is the
  deterministic source; rendered GIFs may vary by font/environment, so CI verifies the render runs
  rather than asserting byte-equality). Update the README `See It Work` to this example.
- **C2-4** — build a SARIF projection of the enforcement decisions for the Security tab, as a named
  slice with its own contract and test (this projection is new for these carriers, not an existing
  output), plus a sample-repo failing/green PR pair via the GitHub Action.
- **C2-5** — export the scenario as a reusable contract-fixture corpus, using the same fixture
  machinery as the existing carrier contracts.

## Verification

Each slice is a small reviewable PR. C2-1/C2-2 must run offline and produce the exact reason codes
deterministically; C2-3's CI smoke-test must regenerate the render without error; C2-5's fixtures
must match the producer output (regenerate-and-compare, like the existing contract fixtures).
