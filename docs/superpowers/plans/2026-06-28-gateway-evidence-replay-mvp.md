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

## Implementation Steps

- [ ] Add `crates/gateway-evidence-replay` to the workspace.
- [ ] Create `Cargo.toml` with package metadata, workspace lints, a `gateway-evidence-replay` binary, and dependencies on workspace `clap`, `serde`, `serde_json`, and `chrono`.
- [ ] Implement `src/schema.rs`:
  - closed `EvidenceBundle`, `Claim`, `Policy`, and `Evidence` structs with `#[serde(deny_unknown_fields)]`;
  - enums for `Coverage`, `SourceClass`, `Status`, `Reason`, `Ceiling`, and `NonClaim`;
  - strict non-empty string validation;
  - strict UTC timestamp parsing for `YYYY-MM-DDTHH:MM:SSZ`;
  - required `profile == "gateway-path.v0"`.
- [ ] Implement `src/replay.rs`:
  - coverage must be complete to confirm;
  - `signature_verified` and `runtime_measurement_verified` must both be true to proceed;
  - stale or missing attestation freshness returns `incomplete`;
  - missing stream commitment returns `incomplete`;
  - mismatches return sorted reason classes and `path_mismatch`;
  - clean evidence returns `path_verified` with the source-class ceiling.
- [ ] Implement `src/lib.rs` exports for `verify_bundle`, result types, and constants.
- [ ] Implement `src/main.rs`:
  - `verify <path> --format gateway-path.v0 --json`;
  - file-read failure is a CLI error;
  - JSON parse or schema failure emits an `invalid` verdict with `malformed_input`;
  - `--format` mismatch emits `invalid` with `malformed_input`;
  - output is deterministic JSON.
- [ ] Port the 13 lab vectors as fixtures under `crates/gateway-evidence-replay/fixtures/gateway-path-v0/`, adding the required `profile` field.
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
