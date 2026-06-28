# Gateway Evidence Replay MVP Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a standalone `gateway-evidence-replay` CLI/library that verifies `gateway-path.v0` evidence bundles and emits bounded replay verdicts.

**Architecture:** Add one isolated Rust workspace crate under `crates/gateway-evidence-replay`. The crate owns a strict `gateway-path.v0` schema, a deterministic replay engine, a JSON CLI, fixtures ported from the lab probe, and package-local tests. It does not integrate into `assay` CLI in this increment and does not perform gateway enforcement, provider verification, or TEE root verification.

**Tech Stack:** Rust 2021 workspace crate, `serde`/`serde_json` for closed schema and output, `clap` for the standalone CLI, `chrono` for strict UTC timestamp parsing, package-local fixtures and integration tests.

---

## Context

The lab probe `gateway-path-evidence-replay-2026-06` found a narrow Gate-D candidate: retained gateway-path evidence can be replayed deterministically to distinguish a verified path from route substitution, hidden fallback, endpoint or policy mismatch, missing stream evidence, stale attestation, and malformed evidence.

SOTA positioning as of June 2026:

- EBGP-style gateway evidence makes the path itself a first-class evidence surface.
- BIV-style declared-vs-actual auditing owns the broad pre-install classifier space, so this MVP must not become BIV-lite.
- The defensible product slice is the reproducible replay layer: input facts in, bounded verdict plus reason classes out.

## Non-Goals

- No production gateway enforcement.
- No network access or provider calls.
- No cryptographic signature verification or TEE root verification; `signature_verified` and `runtime_measurement_verified` are input facts.
- No response-truth, provider-honesty, safety, or compliance verdict.
- No integration into `assay` CLI in this increment.

## Claim ceiling (thesis-led experimental MVP)

This MVP lands as a deliberate whitespace bet, not a demand-proven product. This is the first time a lab finding
becomes product code in Assay, so the bar is higher than "tests green"; the merge is a thesis-led product-gate
decision, recorded here and in the PR body:

- This is an experimental CLI/library MVP.
- Demand signal not yet proven.
- Not BIV-lite (arXiv 2605.11770 owns the broad declared-vs-actual audit space; this stays narrow).
- Not an EBGP replacement (arXiv 2606.22560 owns the attested gateway-runtime mechanism layer).
- It implements the replay-verdict layer over RETAINED gateway-path evidence: input facts in, a bounded verdict
  (`path_verified` / `path_mismatch` / `incomplete` / `invalid`) plus reason classes out, runnable by a third
  party off the gateway runtime.

The wedge is exactly what EBGP does not provide: a portable, deterministic, bounded-verdict replay corpus/CLI a
relying party can run on retained evidence. Capture integration, conformance packs, and any public/external
surface remain separate Gate-D decisions (see Follow-Up).

## Output Contract

CLI:

```bash
gateway-evidence-replay verify evidence.json --format gateway-path.v0 --json
```

JSON result:

```json
{
  "profile": "gateway-path.v0",
  "status": "path_verified",
  "ceiling": "observed_in_path",
  "reasons": [],
  "non_claims": [
    "not_gateway_enforcement",
    "not_provider_honesty",
    "not_response_truth",
    "not_tee_root_verification",
    "not_safety_or_compliance"
  ]
}
```

Statuses:

- `path_verified`
- `path_mismatch`
- `incomplete`
- `invalid`

Reason classes:

- `route_substitution`
- `route_not_allowed`
- `fallback_mismatch`
- `fallback_not_allowed`
- `endpoint_mismatch`
- `policy_hash_mismatch`
- `stream_commitment_mismatch`
- `attestation_stale`
- `attestation_freshness_missing`
- `stream_evidence_missing`
- `evidence_not_verified`
- `coverage_not_complete`
- `malformed_input`
- `unknown_source_class`

## Precedence rules (v0 contract)

The verifier evaluates in this fixed order; the FIRST matching rule decides the verdict. This is the hardened v0
semantic from the lab repair (`gateway-path-evidence-replay-2026-06`, 17 vectors / 9 tests). The MVP must match
it exactly so the DoR and the product tell the same contract.

1. **Shape is invalid.** A non-object input, a missing/empty required field, or a present-but-unparsable
   `attestation_valid_until` is `invalid` / `malformed_input`. A malformed timestamp is a SHAPE error, not an
   incompleteness.
2. **Unknown provenance fails early.** An unrecognized `source_class` is `invalid` / `unknown_source_class`
   BEFORE any path-content evaluation, even when the route visibly mismatches.
3. **Unverified or stale evidence is incomplete.** `signature_verified` / `runtime_measurement_verified` not
   both true, or missing/stale attestation freshness, returns `incomplete` - the observation is not trusted
   enough to refute.
4. **Observed contradictions refute, regardless of coverage.** Route substitution, disallowed route, fallback
   mismatch/disallowed fallback, endpoint mismatch, policy-hash mismatch, or stream-commitment mismatch returns
   `path_mismatch`, including under partial coverage.
5. **Missing stream evidence is incomplete.** No stream commitment returns `incomplete` (stream evidence is
   load-bearing); a mismatched stream commitment is `path_mismatch` under rule 4.
6. **Confirmation requires complete coverage.** Only a clean path under `coverage == complete` returns
   `path_verified`, at the source-class ceiling. A clean partial/absent bundle is `incomplete`.

The load-bearing rule: **partial coverage can refute but never confirm.** A partial bundle with an observed
contradiction is `path_mismatch`, not `incomplete`.

## Implementation Steps

- [ ] Add `crates/gateway-evidence-replay` to the workspace.
- [ ] Create `Cargo.toml` with package metadata, workspace lints, a `gateway-evidence-replay` binary, and dependencies on workspace `clap`, `serde`, `serde_json`, and `chrono`.
- [ ] Implement `src/schema.rs`:
  - closed `EvidenceBundle`, `Claim`, `Policy`, and `Evidence` structs with `#[serde(deny_unknown_fields)]`;
  - enums for `Coverage`, `SourceClass`, `Status`, `Reason`, `Ceiling`, and `NonClaim`;
  - strict non-empty string validation;
  - strict UTC timestamp parsing for `YYYY-MM-DDTHH:MM:SSZ`;
  - required `profile == "gateway-path.v0"`.
- [ ] Implement `src/replay.rs` (follow the Precedence rules section exactly, in that order):
  - shape failure, including a present-but-unparsable `attestation_valid_until`, returns `invalid` / `malformed_input`;
  - an unrecognized `source_class` returns `invalid` / `unknown_source_class` BEFORE any path-content evaluation;
  - `signature_verified` and `runtime_measurement_verified` must both be true, else `incomplete` / `evidence_not_verified`;
  - missing or stale attestation freshness returns `incomplete`;
  - observed contradictions return sorted reason classes and `path_mismatch`, regardless of coverage (partial can still refute);
  - missing stream commitment returns `incomplete`;
  - confirmation requires `coverage == complete`: a clean complete bundle returns `path_verified` at the source-class ceiling; a clean partial/absent bundle is `incomplete`.
- [ ] Implement `src/lib.rs` exports for `verify_bundle`, result types, and constants.
- [ ] Implement `src/main.rs`:
  - `verify <path> --format gateway-path.v0 --json`;
  - file-read failure is a CLI error;
  - JSON parse or schema failure emits an `invalid` verdict with `malformed_input`;
  - `--format` mismatch emits `invalid` with `malformed_input`;
  - output is deterministic JSON.
- [ ] Port the 17 lab vectors as fixtures under `crates/gateway-evidence-replay/fixtures/gateway-path-v0/`, adding the required `profile` field. The lab probe is 17 vectors / 9 tests after the precedence hardening; include the partial-coverage-refute, unknown-source-early, and malformed-attestation-invalid vectors.
- [ ] Add package tests:
  - all fixtures reproduce expected status, ceiling, and reasons;
  - the CLI emits JSON for a clean fixture;
  - unknown fields and malformed timestamps fail closed to `invalid`;
  - claim guard ensures non-claims remain present and no provider-honesty or response-truth claim appears.
- [ ] Add a small crate README documenting scope, CLI use, claim ceiling, and non-claims.

## Verification

Run:

```bash
cargo fmt --all
cargo test -p gateway-evidence-replay
cargo run -p gateway-evidence-replay -- verify crates/gateway-evidence-replay/fixtures/gateway-path-v0/clean-route.json --format gateway-path.v0 --json
cargo clippy -p gateway-evidence-replay -- -D warnings
```

Expected:

- all package tests pass;
- the CLI demo returns `path_verified` and `observed_in_path`;
- malformed evidence returns `invalid`, never a panic;
- no workspace product integration is touched.

## Follow-Up After This MVP

- Add Assay capture integration only behind a separate Gate-D decision.
- Add Harness conformance packs only after the CLI/library contract is stable.
- Revisit external reproduction only when a named external reproducer or public bundle surface exists.
