# Gateway Evidence Replay Gate-D Unlock Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Unlock a bounded Gate-D decision for `gateway-evidence-replay` by building a perfect internal demo that proves the MVP's value on retained gateway-path evidence, then using that demo to decide whether Assay capture integration is justified.

**Architecture:** Use a demo-led two-stage gate. Stage 1 adds a polished but offline replay demo pack around the already-merged standalone CLI: four fixed scenarios, one digest-pinned manifest, one expected transcript, tamper checks, and one decision checklist. Stage 2, only after the demo produces an explicit Gate-D go with a named producer and consumer, adds a facts-only Assay capture adapter that emits `gateway-path.v0` bundles consumable by the replay crate. The replay crate remains the verdict consumer; Assay capture emits facts only.

**Tech Stack:** Rust workspace crates (`gateway-evidence-replay`, `assay-evidence`), `serde`/`serde_json`, package-local fixtures and integration tests, existing `EvidenceEvent` payloads, `cargo test`, and manual SOTA re-checks against AGEF-style formats, replay-verdict-channel work, AEX, Auditable Agents, EBGP, BIV, and runtime enforcement incumbents before any product-facing merge.

---

## Context

The standalone `gateway-evidence-replay` MVP is already merged in Assay as a thesis-led experimental CLI/library. It verifies `gateway-path.v0` evidence bundles and emits bounded verdicts:

- `path_verified`
- `path_mismatch`
- `incomplete`
- `invalid`

The lab repair fixed the v0 precedence contract:

- Partial coverage can refute but never confirm.
- Unknown `source_class` fails early as `invalid`.
- Malformed `attestation_valid_until` is a shape error, not an incomplete evidence state.
- Confirmation requires clean evidence, complete coverage, freshness, and a recognized source class.

The product claim ceiling remains narrow:

- Not provider honesty.
- Not response truth.
- Not gateway enforcement.
- Not TEE root verification.
- Not safety or compliance.

SOTA as of June 2026 supports this narrow wedge:

- EBGP (`arXiv:2606.22560`) owns the attested gateway-runtime mechanism layer and fail-closed gateway-side validation, but does not ship a portable, deterministic, bounded-verdict replay corpus or CLI for relying parties to run off the gateway runtime.
- AEX (`arXiv:2603.14283`) is a closer neighbor than EBGP for multi-hop attestation and provenance in LLM APIs; treat it as a required Stage 1 incumbent check.
- Auditable Agents (`arXiv:2604.05485`) frames the post-deployment audit question and the reconstruct/check/attribute vocabulary; treat it as a required Stage 1 positioning check.
- AGEF-style portable evidence formats and replay-verdict-channel work are the closest possible incumbents. They may be complementary session-format layers, but they must be checked before capture work starts.
- BIV (`arXiv:2605.11770`) owns the broad declared-vs-actual skill audit space at scale, so this line must not become BIV-lite.
- Vigil (`arXiv:2606.26524`) owns runtime enforcement of agent behavior policies, so this line must not become a runtime policy engine.

Gate-D is therefore not unlocked by green CI alone. Green CI only proves the merged MVP is internally sound. Gate-D unlock requires a demo that makes the product value visible: a relying party can replay retained evidence without trusting the gateway runtime and gets a bounded verdict that refuses to overclaim.

## Decision To Unlock

Gate-D unlocks only if all four gates are green:

1. **Perfect demo gate:** A short internal demo shows the four verdict classes on realistic retained evidence, with a clear before/after story, digest-pinned inputs, tamper evidence, and no live network dependency. The demo must end with a yes/no decision on whether facts-only capture is worth building.
2. **Incumbent replayability gate:** Re-check the nearest replay-verdict neighbors before implementation: AGEF-style portable evidence formats, replay-verdict-channel work, AEX, Auditable Agents, EBGP, BIV, and Vigil. If an incumbent now ships a portable bounded-verdict replay corpus/CLI that covers this slice, pivot to reproduction and delta-read instead of building a competing Assay capture path.
3. **Facts-only capture gate:** Assay emits `gateway-path.v0` facts but does not emit the replay verdict. The replay crate computes `path_verified` / `path_mismatch` / `incomplete` / `invalid`.
4. **Claim-ceiling gate:** Docs, tests, and output keep the same five non-claims. No provider honesty, response truth, safety, compliance, gateway enforcement, or TEE-root claim slips into the capture path.

If any gate is red, stop and keep the MVP as a standalone experimental replay tool.

## Perfect Demo Shape

The demo is the Gate-D unlock artifact. It should be runnable in under five minutes by a reviewer who has not followed the lab arc.

### Demo Story

The opening sentence:

> "Here are four retained gateway evidence bundles. We do not ask you to trust the gateway, the provider, or an LLM summary. We replay the bytes and produce a bounded verdict."

The demo then runs four cases:

| Case | Evidence story | Expected verdict | Product point |
| --- | --- | --- | --- |
| A | Clean route, fresh attestation, complete coverage | `path_verified` | Replay can confirm a clean retained path at the source-class ceiling. |
| B | Partial coverage but visible route substitution | `path_mismatch` | Partial evidence can still refute; it just cannot confirm. |
| C | Clean path but stale attestation | `incomplete` | Integrity-looking evidence is not enough when freshness is missing. |
| D | Unknown source class | `invalid` | Provenance defects fail closed before content is trusted. |

The closing sentence:

> "This does not prove provider honesty or response truth. It proves the retained gateway-path evidence is, or is not, sufficient for this bounded replay claim."

### Demo Files

Stage 1 creates:

- `docs/gateway-evidence-replay/GATE-D-DEMO.md`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/clean-route.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/partial-route-substitution.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/stale-attestation.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/unknown-source.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/expected.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/manifest.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/manifest-sha256.txt`
- `crates/gateway-evidence-replay/tests/demo_fixtures.rs`
- `crates/gateway-evidence-replay/tests/demo_tamper.rs`

Optional only if the repo already has a fitting script convention:

- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/run-demo.sh`

The demo must not depend on a live provider, a live gateway, a secret, a TEE service, or external network access.

### Demo Integrity Shape

The demo uses the same discipline as the L6 reproduction kit from the lab line. The replay value is not just "four verdicts"; it is "four verdicts over retained bytes whose identity is pinned."

`manifest.json` records:

- `profile: "gateway-path.v0.demo"`
- the source commit used to build the demo
- one SHA-256 digest per fixture
- the SHA-256 digest of `expected.json`
- the version of `gateway-evidence-replay` used by the demo
- `claims: "internal_demo_candidate"`

`manifest-sha256.txt` stores the digest of `manifest.json`.

`demo_tamper.rs` must prove:

- changing a fixture changes its digest and fails the manifest check
- changing `expected.json` changes its digest and fails the manifest check
- changing `manifest.json` changes `manifest-sha256.txt` verification
- a replay result that differs from `expected.json` fails the replay check

If the demo cannot prove tamper evidence, it does not unlock Gate-D. The differentiator is reproducible replay, not a nice transcript.

### Demo Decision Checklist

`GATE-D-DEMO.md` ends with these explicit questions:

1. Who would produce the retained gateway-path evidence?
2. Who would consume the replay verdict?
3. Which source class is realistic for the first capture path?
4. Is the value the replay verdict, or is the team actually asking for enforcement?
5. Is the next step facts-only capture, or should the MVP remain a standalone lab tool?

Only question 5 can unlock Stage 2, and only if questions 1 and 2 name a concrete producer and consumer. "A plausible future user" is not enough.

## Positioning Against Emerging Vocabulary

`gateway-path.v0` is the gateway-path instance of the emerging `replay_verdict` channel: retained evidence goes in, a bounded replay verdict comes out. AGEF-style formats are complementary session/evidence containers; they may carry the evidence that this verifier replays. This plan must stay visibly non-duplicative: format portability and replay verdicts compose, but neither should silently absorb the other.

## Non-Goals

- Do not integrate live gateway calls.
- Do not call providers or infer response truth.
- Do not verify signatures, attestation roots, or Nitro/TEE measurements in this increment. Those remain input facts.
- Do not add admission or enforcement behavior.
- Do not publish a public corpus or crates.io package.
- Do not add broad declared-vs-actual skill auditing.
- Do not rename the MVP into a general gateway security product.

## Current Anchors In The Codebase

`gateway-evidence-replay` owns the replay contract:

- `crates/gateway-evidence-replay/src/schema.rs`
- `crates/gateway-evidence-replay/src/replay.rs`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/vectors.json`
- `crates/gateway-evidence-replay/tests/gateway_path_v0.rs`

Assay already has the right evidence envelope shape:

- `crates/assay-evidence/src/types.rs`
- `EvidenceEvent.payload: serde_json::Value`
- `EvidenceEvent.semantic_digest: Option<String>`
- `EvidenceEvent.digest_profile: Option<String>`

Important constraint: `semantic_digest` / `digest_profile` are soft correlation metadata and excluded from `content_hash`. Do not use them as the hard replay integrity root. For gateway-path bundles, the evidence payload itself is what the replay crate consumes.

## Stage 1: Perfect Demo And Gate-D Unlock Pack

This stage adds documentation and fixtures only. It does not make Assay emit gateway-path evidence.

### Deliverables

- `docs/gateway-evidence-replay/GATE-D-UNLOCK.md`
- `docs/gateway-evidence-replay/GATE-D-DEMO.md`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/README.md`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/clean-route.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/partial-route-substitution.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/stale-attestation.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/unknown-source.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/expected.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/manifest.json`
- `crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/manifest-sha256.txt`
- `crates/gateway-evidence-replay/tests/demo_fixtures.rs`
- `crates/gateway-evidence-replay/tests/demo_tamper.rs`

### Demo Fixture Rules

Each demo fixture must:

- Use `profile: "gateway-path.v0"`.
- Carry realistic route, endpoint, policy hash, stream commitment, freshness, coverage, and source class fields.
- Be synthetic or redacted; no provider keys, customer prompts, secrets, PII, or live endpoint credentials.
- Include the expected verdict in `expected.json` and in the test, not in the evidence payload.
- Exercise one demo-relevant scenario:
  - Clean route replay.
  - Partial coverage with visible route substitution.
  - Stale attestation.
  - Unknown source class.

The demo fixtures must not claim that demand is proven. They are a Gate-D decision pack: enough to decide whether capture integration is worth building.

### Test Shape

Add `crates/gateway-evidence-replay/tests/demo_fixtures.rs`:

```rust
use std::fs;

use gateway_evidence_replay::schema::{Reason, Status};
use gateway_evidence_replay::verify_json_str;

#[test]
fn demo_fixtures_replay_expected_verdicts() {
    let cases = [
        ("clean-route.json", Status::PathVerified, vec![]),
        (
            "partial-route-substitution.json",
            Status::PathMismatch,
            vec![Reason::RouteSubstitution],
        ),
        ("stale-attestation.json", Status::Incomplete, vec![Reason::AttestationStale]),
        ("unknown-source.json", Status::Invalid, vec![Reason::UnknownSourceClass]),
    ];

    for (name, expected_status, expected_reasons) in cases {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/fixtures/gateway-path-v0/demo/"
        )
        .to_owned()
            + name;
        let body = fs::read_to_string(&path).expect("read demo fixture");
        let got = verify_json_str(&body);
        assert_eq!(got.status, expected_status, "{name}");
        assert_eq!(got.reasons, expected_reasons, "{name}");
    }
}
```

Keep this test intentionally small. The canonical behavior still lives in `vectors.json`; the demo pack proves product shape, not exhaustive semantics.

### Stage 1 Verification

Run:

```bash
cargo test -p gateway-evidence-replay
cargo run -p gateway-evidence-replay -- verify crates/gateway-evidence-replay/fixtures/gateway-path-v0/demo/clean-route.json --format gateway-path.v0 --json
cargo fmt --check
cargo clippy -p gateway-evidence-replay --all-targets -- -D warnings
```

Expected clean-route output:

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

### Stage 1 Exit Criteria

- Demo fixtures replay correctly.
- `GATE-D-DEMO.md` is understandable without the lab history.
- The demo shows all four verdict classes.
- Manifest and expected-output digests are pinned.
- Tamper tests fail closed.
- `GATE-D-UNLOCK.md` records the four gates and their current state.
- The SOTA/incumbent check is explicitly dated and covers AGEF-style formats, replay-verdict-channel work, AEX, Auditable Agents, EBGP, BIV, and Vigil.
- No Assay capture code is added.
- No public release or external reproduction claim is made.

If this stage merges, the result is: "Assay has a demo-shaped replay fixture pack and can now decide whether to build facts-only capture."

## Stage 2: Facts-Only Assay Capture Adapter

Start this stage only after Stage 1 merges and the Gate-D decision is explicit.

### Deliverables

- `crates/assay-evidence/src/gateway_path.rs`
- `crates/assay-evidence/src/types/tests.rs` additions or a new `crates/assay-evidence/tests/gateway_path_v0.rs`
- Optional module export from `crates/assay-evidence/src/lib.rs`

Do not modify the top-level Assay CLI in this stage unless a named consumer needs it.

### Data Model

Introduce a typed facts-only payload builder:

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GatewayPathEvidenceV0 {
    pub profile: String,
    pub request_id: String,
    pub coverage: String,
    pub source_class: String,
    pub now: String,
    pub claim: GatewayPathClaimV0,
    pub policy: GatewayPathPolicyV0,
    pub evidence: GatewayPathObservedV0,
}
```

This type may live in `assay-evidence`, but it must not compute a replay verdict. It only serializes the facts that `gateway-evidence-replay` already knows how to consume.

The emitted event must be hard-bound through Assay's existing `content_hash` path. Do not rely on `semantic_digest` or `digest_profile`; those are soft correlation metadata. Stage 2 must include a test proving that mutating the `gateway-path.v0` payload changes the event's hard `content_hash` after bundle normalization or explicit content-hash computation.

Add a constructor that creates an `EvidenceEvent`:

```rust
impl GatewayPathEvidenceV0 {
    pub fn into_event(
        self,
        source: impl Into<String>,
        run_id: impl Into<String>,
        seq: u64,
    ) -> Result<EvidenceEvent, serde_json::Error> {
        let payload = serde_json::to_value(self)?;
        Ok(EvidenceEvent::new(
            "com.assay.gateway_path.v0",
            source,
            run_id,
            seq,
            payload,
        ))
    }
}
```

This is the key line: Assay emits an `EvidenceEvent` whose payload is replayable. It does not emit `path_verified`.

### Stage 2 Tests

Add tests that:

- Build `GatewayPathEvidenceV0`.
- Convert it into `EvidenceEvent`.
- Compute or normalize the event's hard `content_hash`.
- Extract `event.payload`.
- Pass the payload into `gateway_evidence_replay::verify_json_value`.
- Assert the expected replay verdict.
- Assert the event itself does not contain replay status fields outside payload.
- Mutate the payload and assert the hard `content_hash` changes.

Example:

```rust
#[test]
fn gateway_path_event_payload_replays_but_event_has_no_verdict() {
    let evidence = clean_gateway_path_evidence();
    let event = evidence
        .into_event("assay://gateway-test", "run-1", 1)
        .expect("event");

    assert_eq!(event.type_, "com.assay.gateway_path.v0");
    assert!(event.payload.get("profile").is_some());
    assert!(event.payload.get("status").is_none());
    assert!(event.payload.get("provider_honest").is_none());

    let got = gateway_evidence_replay::verify_json_value(event.payload.clone());
    assert_eq!(got.status, gateway_evidence_replay::schema::Status::PathVerified);
}
```

### Stage 2 Verification

Run:

```bash
cargo test -p assay-evidence gateway_path
cargo test -p gateway-evidence-replay
cargo fmt --check
cargo clippy -p assay-evidence -p gateway-evidence-replay --all-targets -- -D warnings
```

If adding a dependency from `assay-evidence` tests to `gateway-evidence-replay` creates an undesirable dependency edge, keep the replay assertion in a workspace-level integration test or compare the serialized payload byte-for-byte with an existing fixture instead. Do not create a production dependency cycle just to make the test convenient.

### Stage 2 Exit Criteria

- Assay can emit a facts-only `EvidenceEvent` carrying a `gateway-path.v0` payload.
- The payload replays through `gateway-evidence-replay`.
- The payload is bound to the event's hard `content_hash`.
- The event itself contains no replay verdict.
- The five non-claims remain in the replay result, not in the capture event.
- No live gateway or provider integration exists yet.
- A named producer and named consumer for this evidence are recorded in the Stage 2 PR body.

If this stage merges, the result is: "Assay can capture a replayable gateway-path evidence fact."

## Stage 3: Consumer Surface Decision

Start this stage only if there is a named user or integration that needs to produce or consume gateway-path evidence.

Possible surfaces:

- A subcommand that writes a gateway-path fixture from a retained evidence JSON.
- A bundle export path that includes `gateway-path.v0` evidence events.
- A documentation page showing how an external party runs `gateway-evidence-replay` against an Assay-exported event payload.

Do not build a general gateway product, policy engine, or provider proxy in this stage.

## Kill Criteria

Stop the line if any of these become true:

- AGEF, replay-verdict-channel work, AEX, Auditable Agents, EBGP, BIV, Vigil, or adjacent work ships a portable bounded-verdict replay corpus/CLI that covers this slice.
- A named producer and named consumer do not materialize after Stage 1.
- Capture integration requires Assay to make provider-honesty, response-truth, enforcement, compliance, or TEE-root claims.
- The implementation needs live provider calls to be meaningful.
- The replay crate starts absorbing broad declared-vs-actual audit behavior.

## Work Sequence

- [ ] Re-run a primary-source SOTA check for AGEF-style formats, replay-verdict-channel work, AEX, Auditable Agents, EBGP, BIV, and runtime enforcement before opening the Stage 1 PR.
- [ ] Add `docs/gateway-evidence-replay/GATE-D-UNLOCK.md` with the four gates and current decision state.
- [ ] Add `docs/gateway-evidence-replay/GATE-D-DEMO.md` with the four-case script and decision checklist.
- [ ] Add the demo fixture directory and four synthetic or redacted fixtures.
- [ ] Add `demo/expected.json`.
- [ ] Add `demo/manifest.json` and `demo/manifest-sha256.txt`.
- [ ] Add `tests/demo_fixtures.rs`.
- [ ] Add `tests/demo_tamper.rs`.
- [ ] Verify `cargo test -p gateway-evidence-replay`.
- [ ] Verify the CLI against the clean demo fixture.
- [ ] Verify at least one tamper mutation fails before replay trust.
- [ ] Commit Stage 1 separately with a message like `docs: add gateway replay gate-d unlock pack`.
- [ ] Ask for explicit Gate-D approval before Stage 2.
- [ ] If approved, add `GatewayPathEvidenceV0` in `assay-evidence`.
- [ ] Add facts-only `EvidenceEvent` construction tests.
- [ ] Add replay compatibility tests without creating a production dependency cycle.
- [ ] Add hard `content_hash` mutation tests for the emitted payload.
- [ ] Commit Stage 2 separately with a message like `feat: add gateway path evidence event shape`.
- [ ] Stop before any CLI/product surface unless a named consumer asks for it.

## Final Review Checklist

- [ ] No claim that demand is proven unless a named producer and consumer are recorded after the demo.
- [ ] No claim that EBGP is replaced.
- [ ] No claim that BIV or Vigil are superseded.
- [ ] No claim that AGEF-style formats or AEX are replaced.
- [ ] No verdict emitted by Assay capture.
- [ ] `gateway-evidence-replay` remains the only verdict producer.
- [ ] Demo inputs and expected outputs are digest-pinned.
- [ ] Tamper checks fail closed.
- [ ] Stage 2 payloads bind to hard `content_hash`.
- [ ] Non-claims remain visible in replay output.
- [ ] Partial coverage still refutes but never confirms.
- [ ] Unknown source class still fails early.
- [ ] Malformed attestation freshness is still `invalid`.
- [ ] All new fixtures are synthetic or redacted.
- [ ] No secrets, keys, endpoints, customer data, or provider credentials in fixtures.

## Recommended Decision

Do Stage 1 next. It is the smallest meaningful Gate-D unlock step: it turns the thesis-led MVP into a perfect internal demo without committing Assay to capture integration yet.

Do not start Stage 2 until Stage 1 produces a clear go/no-go and the user explicitly approves Gate-D for facts-only capture.
