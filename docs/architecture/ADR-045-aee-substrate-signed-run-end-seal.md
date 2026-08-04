# ADR-045: AEE-compatible substrate-signed run-end seal primitive

- Status: Proposed
- Date: 2026-08-04
- Supersedes: none
- Amends: ADR-042 (evidence-first positioning), ADR-043 (evidence-chain integrity invariants)
- Applies: ADR-044 (attestation subject is the artifact, not the semantic chain)
- Related: #1998, #1997, in-toto/attestation#570

## Context

ADR-042 positions Assay as evidence-first: a record states what it can carry, integrity never upgrades meaning, and Assay refuses broad trust, safety, compliance, whole-action, and provider-outcome claims. ADR-043 makes those boundaries testable: untrusted evidence is consumed under bounded, fail-closed rules, emitted claims are constrained, and policy or authorization absence never becomes a clean result. ADR-044 also applies: any future AEE statement subject must identify the executed artifact by artifact digest, not a semantic run chain or Assay internal evidence root.

While in-toto AEE remains unaccepted, "AEE-compatible" in this ADR means shape-compatible with the current v0.7 draft and explicitly experimental for external export.

The AEE v0.7 fixture spike in #1997 tested Assay's existing runtime/proxy evidence carriers against the current Adversarial Execution Evidence shape in in-toto/attestation#570. It deliberately built the `sealed` record first, then emitted and consumed a synthetic two-vantage run:

- one proxy-deny row derived from `assay.denied_call_observation.v0`;
- one Landlock/TCP-connect row derived from `assay.enforcement_health.v1` active-with-probe;
- one synthetic substrate descriptor with two collection paths;
- fixture arming, interception, and sealed records;
- negative controls for missing seal, defective unreferenced seal, artifact/proxy overclaim, reconstructed/intercepted mismatch, and run-population overclaim.

The spike result was useful precisely because it did not become production support. It showed that the AEE field shape is coherent enough for an Assay-shaped two-vantage fixture, and that Assay's current production carriers stop short of the primitive AEE needs most: a substrate-signed run-end seal.

Current Assay carriers provide partial inputs:

- `assay.enforcement_health.v1` can report Landlock active/probe state, including whether a denied TCP-connect probe was blocked before a listener was reached.
- `assay.denied_call_observation.v0` can report a caller-visible proxy denial observation.
- enforcement counters such as allow/block counts are enforcement event counts, not observation-drop accounting.

None of these is a substrate-signed run-end record over:

- still-armed state;
- observation-drop accounting;
- observed set;
- observed attacks;
- run binding.

A production AEE exporter before this primitive would overstate Assay's evidence strength. The exporter would be able to assemble AEE-shaped JSON, but the strongest required record would remain producer-synthesized rather than substrate-observed and signed.

## Requirements

### Functional requirements

1. Assay MUST be able to produce a run-end sealed payload for a bounded run.
2. The sealed payload MUST bind to the run via an AEE-compatible run binding.
3. The sealed payload MUST be signed by an observation key whose role is explicit.
4. The sealed payload MUST include, at minimum:
   - `aeeKind = sealed`;
   - `aeeVersion`;
   - `aeeRunBinding`;
   - `aeeMethod`;
   - `aeePostureDigest`;
   - `aeeStillArmed`;
   - `aeeDropCount`;
   - `aeeDropBound`;
   - `aeeObservedSet`;
   - `aeeObservedAttacks`.
5. The checker MUST reject a substrate row that lacks valid sealed coverage.
6. The checker MUST constrain every carried covering-kind record, whether or not any row references it.
7. The checker MUST reject mismatched run binding, observed set, observed attacks, still-armed state, and drop accounting.
8. The implementation MUST preserve the distinction between policy decisions, observations, and outcomes.
9. The implementation MUST keep non-claims explicit.

### Non-functional requirements

1. Security: all evidence inputs are hostile; malformed input fails closed.
2. Canonicality: signed bytes are exact bytes, not reserialized object-model output.
3. Interoperability: AEE/DSSE semantics must be followed where used, but production support must not be claimed while AEE remains draft unless the output is explicitly experimental.
4. Maintainability: run-binding derivation exists in one implementation path used by producer/checker.
5. Evolvability: the first slice must not prevent later per-collection-path observation keys.
6. Operability: failure modes must be diagnosable without turning missing evidence into a clean result.

## Constraints

- ADR-042 stop list remains normative:
  - no scalar trust score;
  - no whole-action verdict;
  - no generic agent identity, delegation, or federation layer;
  - no provider-outcome verification;
  - no detector catalogue or broad MCP scan;
  - no compliance or safe-agent claim;
  - no certification or partnership status without a checkable basis.
- Production Assay currently has no AEE-compatible substrate-signed seal primitive.
- `blocked_count` / `allowed_count` MUST NOT be relabelled as AEE drop accounting.
- ProtoJSON or generated bindings MUST NOT be used as a canonical signing surface.
- A fixture key or fixture signer MUST NOT be accepted as production substrate evidence.
- Any production exporter MUST remain blocked until the sealed primitive exists.
- Any future AEE statement subject MUST follow ADR-044: the subject identifies the executed artifact, not the semantic evidence chain.

## Decision

Assay will introduce an AEE-compatible substrate-signed run-end seal primitive, starting with the Landlock/TCP-connect collection path.

The first production slice will implement **Landlock-first sealing**, not full AEE export.

The design will use a single observation key role for the first implementation, but all payloads will carry explicit collection-path identity so the design remains compatible with later per-collection-path keys.

Any future production AEE statement exporter remains blocked until the following are true:

1. Landlock/TCP-connect can emit a signed run-end sealed record.
2. The run binding is derived by a single shared function.
3. The checker rejects every seal failure listed in this ADR.
4. The documentation states the non-claims prominently.
5. Fixture-only signing cannot be mistaken for production observation signing.

### Observation key policy

The production seal signer is a substrate observation key role, not a policy-decision key.

For the first slice:

- fixture keys are rejected by construction in production paths;
- policy-decision keys MUST NOT sign substrate observation records;
- the checker derives structural validity without trusting keys;
- consumer policy separately decides whether the key is trusted as a substrate observation key;
- a valid signature from an untrusted key is structurally valid but not credited as attested substrate evidence;
- key IDs are lookup hints, not authority;
- key rotation and compromise handling are outside the first implementation, but MUST be named before any stable AEE export is exposed.

### Key scope binding

Consumer policy MUST bind accepted observation keys to at least:

- substrate digest or substrate identity;
- collection path;
- environment class, if applicable;
- key role.

A signature is credited as attested substrate evidence only if:

1. the signature verifies;
2. the key is trusted by consumer policy;
3. the key's trusted scope includes the payload's `assayCollectionPath`;
4. the key's trusted scope is compatible with the statement's substrate descriptor.

A trusted key outside scope is structurally valid but not credited as attested substrate evidence.

## Seal payload shape

The first production payload is a signed envelope whose payload is canonical JSON encoded as UTF-8. The signed bytes are the exact UTF-8 bytes of that canonical payload plus the envelope's production signing preimage; consumers verify the envelope before reading any payload fields.

### Production signing envelope

The production seal MUST define its signing surface explicitly before any stable exporter is exposed:

- envelope format: a versioned Assay observation envelope, with any DSSE use called out by name if selected;
- payload type: a producer-owned media type for the seal payload, ending in `+json`;
- payload bytes: RFC 8785 canonical JSON encoded as UTF-8, with duplicate object members rejected before signing or verification;
- signature algorithm: a production asymmetric signature algorithm and key role approved for substrate observation signing; fixture HMAC keys are invalid in production;
- verification rule: verify the envelope signature and key scope over the exact signed bytes before decoding the payload;
- non-normative fixture boundary: `scripts/experiments/aee_spike_lib.py` and its fixture DSSE PAE/HMAC helper are experiment-only and do not define production signing semantics.

Until those details are implemented and tested, this ADR authorizes only the primitive design and fixture/checker work, not stable production AEE export.

Illustrative shape:

```jsonc
{
  "aeeKind": "sealed",
  "aeeVersion": "0.7",
  "aeeRunBinding": "<lowercase sha256 hex>",
  "aeeMethod": "intercepted",
  "aeePostureDigest": "<observationEnvironment.networkPosture.digest.sha256>",
  "aeeStillArmed": true,
  "aeeDropCount": 0,
  "aeeDropBound": 0,
  "aeeObservedSet": "<64-hex digest over emitted interception/examination record leaves>",
  "aeeObservedAttacks": [],
  "assayObservedLabels": ["connect_blocked"],
  "assayCollectionPath": "landlock-tcp-connect",
  "assaySourceSchema": "assay.enforcement_health.v1",
  "assaySealScope": "tcp_connect_landlock_port",
  "assayAttackRowAttributionSource": "assembly-plane",
  "assayNonClaims": [
    "does not prove complete run population",
    "does not prove agent safety",
    "does not prove provider side effects",
    "does not prove independent substrate operation"
  ]
}
```

The empty `aeeObservedAttacks` in this example is deliberate. It is the safe default for a pure Landlock observation path whose signer did not also dispatch the corpus attack. The `aeeObservedSet` value is also deliberate: AEE v0.7 defines it as a digest commitment over emitted `interception` and `examination` record leaves, not as a label array.

### Field interpretation

- `aeeStillArmed`: true only if the relevant enforcement/observation mechanism remained armed at run end. Unknown or failed state is not true.
- `aeeDropCount`: count of observation drops/losses, not policy-blocked events.
- `aeeDropBound`: bound on unobserved/lost observations. The first production slice emits a production AEE-compatible seal only when it can honestly carry zero under its own collection model.
- `aeePostureDigest`: MUST equal the carried `observationEnvironment.networkPosture.digest.sha256`. It is distinct from the AEE v0.7 run-binding `networkPosture` input, which is the digest of the canonicalized carried `networkPosture` object.
- `aeeObservedSet`: AEE v0.7 digest commitment over every emitted `interception` and `examination` record leaf for this run. It is not the observed-label set.
- `aeeObservedAttacks`: lower bound of attacks the substrate can bind to observed events. For the Landlock-first slice this value is empty unless the attack-id-to-observation correspondence is inside the signed substrate boundary.
- `assayObservedLabels`: optional Assay producer vocabulary for labels such as `connect_blocked`; it never substitutes for `aeeObservedSet`.
- `assayCollectionPath`: producer vocabulary to prevent flattening a multi-vantage substrate into one undifferentiated source.
- `assayAttackRowAttributionSource`: producer vocabulary naming whether row-level attack attribution came from the substrate boundary or from assembly. It never upgrades the substrate claim.
- `assayNonClaims`: Assay producer vocabulary for non-claims inside the signed payload. It is not the AEE predicate-level `doesNotAssert` field.

### AEE normative fields vs Assay producer vocabulary

| Payload member | AEE normative? | Meaning |
|---|---:|---|
| `aeeObservedSet` | yes | Digest over emitted `interception` / `examination` record leaves |
| `aeeObservedAttacks` | yes | Lower-bound attack IDs the substrate can attribute to its own observations |
| `aeePostureDigest` | yes | Must equal `observationEnvironment.networkPosture.digest.sha256` |
| `assayObservedLabels` | no | Assay-readable label summary for operators/debugging |
| `assayCollectionPath` | no | Assay producer collection path |
| `assayAttackRowAttributionSource` | no | Assay explanation of row attribution source |
| `assayNonClaims` | no | Assay payload-local non-claims |

Fields beginning with `assay` in the sealed payload are Assay producer vocabulary. AEE consumers may ignore them unless their own policy understands them. They MUST NOT alter AEE structural validity. Any future AEE statement exporter MUST also carry predicate-level `doesNotAssert` for statement-level non-claims; `assayNonClaims` inside the sealed payload is only producer vocabulary and does not weaken required AEE checks.

### Landlock still-armed proof

For the Landlock-first slice, `aeeStillArmed = true` requires one of:

1. a run-end probe executed after corpus injection and before seal signing, proving a denied TCP connect is still blocked; or
2. a documented kernel-level invariant showing the applied Landlock restrictions cannot be relaxed for the sealed subject process scope, plus evidence that the sealed subject scope is the same scope that was restricted.

Start-time `restrict_self_confirmed` alone is not sufficient for a run-end seal unless paired with one of the above.

### Drop accounting decision for first slice

The Landlock-first production slice MUST NOT emit a successful AEE-style sealed record unless it can prove the observation-drop accounting value it carries.

For the first slice, Assay supports only:

- `aeeDropCount = 0`;
- `aeeDropBound = 0`.

Those values may be emitted only when the collection path has no buffered observation channel whose loss can occur outside the process's knowledge, or when such a channel has its own loss counter and that counter is known to be zero at run end. If that condition is not met, the run emits no production AEE-compatible seal and records an Assay failure/non-claim instead. It MUST NOT emit an AEE-looking seal with guessed zero drop accounting.

### Drop-accounting proof sources

For the first Landlock slice, `aeeDropCount = 0` and `aeeDropBound = 0` may be emitted only under one of these explicitly named collection models:

1. **Synchronous probe model**: the only sealed observation is the run-end probe result itself, obtained synchronously, with no intermediate event queue or lossy buffer.
2. **Counted queue model**: every observation channel between event capture and seal builder exposes a loss counter, and every such counter is read at run end and equals zero.

If neither model applies, the run is not seal-eligible.

### Attack attribution boundary

For the Landlock-first slice, the substrate may sign `aeeObservedAttacks` only when one of the following is true:

1. the substrate/runner component that signs the seal also dispatched the attack and holds the attack-id-to-observation correspondence;
2. the observation is matched through a corpus-pinned `expectedPayloads` commitment and the attribution rule being claimed is checkable by the consumer; or
3. the seal carries an empty `aeeObservedAttacks` lower bound and the assembly plane performs row attribution outside the substrate claim.

A Landlock-only observation of a denied connect MUST NOT by itself claim that `NET-CONNECT-BLOCK-001` was observed unless the attack correspondence is inside the signed substrate boundary. The first production slice therefore defaults to an empty `aeeObservedAttacks` set for a pure Landlock seal, while still allowing the later AEE statement assembly to relate records to rows under the weaker assembly-plane boundary.

Assembly-plane attribution may support a weaker row-level binding, but MUST NOT upgrade `basis`, `method`, or evidence tier. A row may use a stronger binding such as `attribution: pinned` only when the corpus `expectedPayloads` and the referenced observation record commitments make that binding checkable by the consumer.

### Observed-attacks validation

`aeeObservedAttacks` is a substrate-signed lower bound, not the complete assembly row set. A seal naming an attack obliges a caught row for that attack; a seal omitting one licenses nothing. Omission is not a clean-row claim and not evidence that the attack was not observed.

For each sealed record:

1. Every attack ID named in `aeeObservedAttacks` MUST correspond to at least one caught row in the carried statement, unless the seal is being validated standalone before statement assembly.
2. A caught row MAY exist whose `attackId` is not named in `aeeObservedAttacks` when `assayAttackRowAttributionSource = "assembly-plane"`.
3. Equality between `aeeObservedAttacks` and caught row attack IDs is required only when `assayAttackRowAttributionSource = "substrate-runner"` or an equivalent signed substrate-dispatch boundary is present.
4. Empty `aeeObservedAttacks` is valid for a pure Landlock seal and means the substrate signs no attack-ID correspondence.

## Options considered

### Option A: Single substrate observation key, collection path in payload

One key signs arming/interception/sealed payloads. Payloads carry `assayCollectionPath` and source schema. Later AEE statement rows may carry their row-level layer/attribution vocabulary, but this primitive does not add an `actualLayer` seal-payload member.

Pros:

- smallest key-management surface;
- fastest path from fixture to production primitive;
- works for one substrate descriptor under one operator;
- keeps collection-path semantics visible in signed bytes.

Cons:

- weak independence story;
- compromise of one key affects all collection paths;
- consumers must not infer independent substrate operation.

### Option B: Per-collection-path observation keys

Proxy and Landlock/LSM records are signed by distinct role keys under one Assay operator.

Pros:

- stronger evidence-tier semantics;
- clearer collection-path attribution;
- better future support for independent assurance.

Cons:

- more complex key lifecycle;
- more complex consumer policy;
- higher implementation cost before the core seal primitive is proven.

### Option C: Landlock-first seal, proxy later

Implement production seal for Landlock/TCP-connect first; keep proxy mapping experimental until the seal primitive is real.

Pros:

- smallest honest production slice;
- Landlock `enforcement_health.v1` is closer to substrate evidence than proxy decision records;
- avoids conflating policy decisions and observations;
- still tests the primitive AEE cares about most.

Cons:

- does not yet answer the full two-vantage production case;
- proxy remains fixture-only in the first slice.

## Chosen option

Choose **Option C** as the first production slice, with payload design compatible with Option B.

The first implementation will produce a Landlock/TCP-connect run-end seal. It will not produce a stable AEE statement exporter. It will produce and check the primitive that a later exporter must require.

## Architecture

### Seal eligibility

A run is **seal-eligible** only when all of the following are true:

1. run binding is derivable from the run's pre-injection inputs;
2. the Landlock/TCP-connect collection path was armed for the run;
3. run-end still-armed state can be established;
4. observation-drop accounting can honestly be represented as `aeeDropCount = 0` and `aeeDropBound = 0` under one of the named proof-source models for the first slice;
5. `aeeObservedSet` digest can be recomputed over emitted `interception` / `examination` record leaves;
6. observed attacks are either substrate-known under the attack attribution boundary above or explicitly empty as a lower-bound claim;
7. run-end still-armed proof is available under the Landlock still-armed proof rule;
8. the signing key is a production substrate observation key role, not a fixture or policy-decision key, and its trusted scope includes the collection path.

A run that is not seal-eligible MUST NOT emit a production AEE-compatible sealed record.

### Components

```text
Run start
  -> run entropy source
  -> run binding preimage builder
  -> arming record producer

Landlock collection path
  -> ruleset/probe/enforcement-health source
  -> observation accumulator
  -> drop accounting source

Run end
  -> still-armed check
  -> observed-set computation
  -> observed-attacks lower-bound computation
  -> seal payload builder
  -> observation signer
  -> sealed record artifact

Checker
  -> verify-then-read record payloads
  -> recompute run binding
  -> recompute observed set / observed attacks
  -> validate drop accounting
  -> fail closed on malformed or missing coverage
```

### Data flow

```text
policy + corpus + subject + substrate + network posture + run entropy
  -> run binding
  -> arming payload
  -> Landlock run/probe observations
  -> run-end seal payload
  -> signed sealed record
  -> checker validation
```

### Trust boundaries

- Fixture signing keys are not trusted in production.
- Production observation signing key is a local substrate observation key, not a policy-decision key.
- Assembly may relate records to corpus attack IDs only within explicitly documented lower-bound limits.
- Consumer trust policy decides whether the signing key is an acceptable substrate observation key; the predicate alone must not infer that.

## Failure modes

| Failure | Required behavior |
|---|---|
| Run binding missing or mismatched | invalid / fail closed |
| Seal missing for substrate row | invalid / fail closed |
| Seal says not still armed | invalid for substrate-intercepted claim |
| Drop accounting non-zero where zero is claimed | invalid |
| Drop accounting cannot be proven zero | no production AEE-compatible seal is emitted; run is not eligible for substrate AEE export |
| `aeeObservedSet` digest mismatch over emitted interception/examination leaves | invalid |
| Seal names attack not supported by caught rows | invalid |
| Seal omits caught row attack under assembly-plane attribution | valid lower-bound, not completeness |
| Seal omits caught row attack under substrate-runner attribution | invalid |
| Start armed but no run-end still-armed proof | no production AEE-compatible seal is emitted |
| Defective unreferenced seal carried | invalid |
| Fixture key used in production path | invalid / configuration error |
| Producer omits all substrate rows | allowed only as weaker statement; must not imply substrate evidence |

## Security and governance analysis

Agentic systems increase risk around privilege, design/configuration, behavior, structural brittleness, and accountability. For Assay, the relevant control is not a natural-language assurance that an agent stayed within bounds; it is a signed, bounded, recomputable runtime evidence chain.

This ADR therefore follows these principles:

1. Least evidence claim: every field states only what the substrate can know.
2. Verify before read: signed payloads are meaningless until signature verification succeeds under consumer policy.
3. Fail closed: malformed, absent, dropped, or inconsistent evidence never becomes clean.
4. Explicit non-claims: run population, agent safety, provider side effects, and independent substrate operation are not implied.
5. One rule, one function: producer/checker share run-binding derivation.

## Capacity estimate

This primitive is per-run, not per-request long-term storage.

Order-of-magnitude expectation for the first slice:

- one arming record per run;
- zero or more interception records per run;
- one sealed record per run;
- small JSON payloads, expected to be kilobytes not megabytes;
- cost dominated by signing, hashing, bounded JSON parsing, and Landlock run-end checks.

No throughput or latency SLA follows from this ADR. Signing latency, Landlock run-end checks, and any observation-drop accounting path require benchmarking with Assay's real Landlock workloads before release claims are made.

## Consequences

### Positive

- Establishes the missing primitive before export.
- Keeps AEE integration aligned with ADR-042/043 evidence boundaries.
- Provides a concrete implementation path for issue #1998.
- Makes later AEE export a composition step rather than a trust upgrade.

### Negative / costs

- Adds key-role and signing lifecycle decisions.
- Requires precise drop-accounting semantics before any clean claim.
- Does not immediately deliver full two-vantage production AEE export.
- May require rework if AEE v0.7 changes before acceptance.

## Review triggers

Revisit this ADR if any of the following occur:

- AEE changes the sealed record requirements or run-binding construction.
- Assay introduces per-collection-path observation keys.
- Proxy path needs production AEE support before Landlock seal is complete.
- Drop accounting cannot be honestly represented under current AEE vocabulary.
- Consumers require independent substrate operation claims that one Assay operator cannot provide.

## Development fixture path policy

Any fixture, checker, or experiment path used while developing this primitive MUST use named fixtures or allowlists, not arbitrary user-provided filesystem paths. This keeps the experiment harness aligned with the same fail-closed posture as production evidence ingestion.

## Validation plan

Before implementation is accepted:

- Add fixtures for valid Landlock seal.
- Add negative fixtures for:
  - missing seal;
  - mismatched run binding;
  - not-still-armed seal;
  - non-zero/inconsistent drop accounting;
  - uncounted observation channel without eligible seal;
  - start-armed but no run-end still-armed proof;
  - bad `aeeObservedSet` digest over emitted interception/examination leaves;
  - label array present without matching `aeeObservedSet` digest;
  - `aeePostureDigest` confused with the run-binding digest of the full `networkPosture` object;
  - seal naming an attack not supported by caught rows;
  - empty observed-attacks lower bound under assembly-plane attribution;
  - equality-required observed-attacks mismatch under substrate-runner attribution;
  - defective unreferenced seal;
- Add property tests that malformed evidence is invalid, not clean.
- Add a smoke command that is fail-closed under shell semantics.
- Ensure no production path accepts fixture signing keys.

## Non-goals

- Full AEE conformance checker.
- AEE conformance claim while the upstream predicate remains draft.
- Stable production AEE exporter in this ADR.
- Proxy-path production AEE support in the first slice.
- Agent safety certification.
- Complete run-population proof.
- Provider side-effect verification.
- Generic agent identity or delegation framework.

## Next implementation slice

Open a branch for issue #1998 with:

1. this ADR as `docs/architecture/ADR-045-aee-substrate-signed-run-end-seal.md`;
2. a small Landlock seal payload type or schema sketch;
3. fixture tests proving the negative controls before production code;
4. no exporter command.

Implementation should start by making the checker reject missing/malformed production seal fixtures before adding producer code.
