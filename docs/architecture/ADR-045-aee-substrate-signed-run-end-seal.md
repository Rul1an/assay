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

**Status (2026-08-06): implemented in `crates/assay-cli/src/aee_seal_envelope.rs`.** The choices above resolve as:

| requirement | choice |
|---|---|
| envelope format | DSSE, named explicitly, over `assay_common::dsse::build_pae` — the workspace's one PAE |
| payload type | `application/vnd.assay.aee-landlock-seal.v1+json` |
| payload bytes | RFC 8785 via `assay_canonical::jcs`, duplicate members rejected by `parse_strict` before signing and before verification |
| signature algorithm | Ed25519; the fixture harness's HMAC key shape cannot verify here |
| verification rule | signature over the PAE of the exact transmitted bytes, then key role, keyid and collection-path scope; no refusal path returns a payload |

RFC 8785 is required even though DSSE's PAE does not need it: the checker *recomputes* `aeeRunBinding` and `aeeObservedSet` from the payload, and a recomputation over non-canonical bytes is not reproducible. Signature stability and recomputability are two requirements, and PAE answers only the first.

This closes production gate 1. Gates 2, 3 and 4 were closed by the shared derivation function, the checker's 24 negative controls, and the enforced payload-local non-claims. Gate 5 — fixture signing cannot be mistaken for production — is structural rather than documentary: the payload type differs, and PAE binds it, so the two sign different bytes for an identical payload.

Still out of scope, and named here rather than left implicit: **key rotation and compromise handling**, which this ADR already requires be named before any stable AEE export is exposed, and **multi-signature envelopes**, which the verifier refuses by count rather than crediting whichever entry it looked at first.

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
| `assaySealedAt` | no | RFC 3339 UTC instant the seal was signed |
| `assaySourceSchema` | no | The Assay record schema the observation came from |
| `assaySealScope` | no | The enforcement boundary this seal speaks for |
| `assayDropProofModel` | no | Which named model licenses the drop accounting |
| `assayDropProofBasis` | no | `checked` or `declared`, per the model above |
| `assayDropChannels` | no | Per-channel loss counters, under the counted-queue model |
| `assayAttackRowAttributionSource` | no | Assay explanation of row attribution source |
| `assayNonClaims` | no | Assay payload-local non-claims |

This table listed four members while `SealPayload` declared ten, for as long as the struct has
existed. It was written against the design and never re-read against the code, which is the drift a
hand-kept table beside a type always develops. Two of the missing six are the drop-proof pair, so
the omission hid the one member that says whether the drop count was verified or asserted.

**Decided 2026-08-07.** This passage previously recorded two count mismatches and parked them as a
contract question: does the fixture checker require the drop-proof members, or does the producer
stop emitting them? Neither. Both readings take for granted that an Assay producer member may be
load-bearing for whether an AEE record is well formed, and the prefix rule below says it may not.

The checker did not obey that rule. `assayDropProofModel` sat in `REQUIRED_SEAL_FIELDS`, was gated
against a closed value set, and the finding it raised was a structural one — so a value only Assay
defines decided AEE structural validity, in the one tool we point at fixtures to prove the
opposite. The resolution now in force:

- **Producer vocabulary never contributes to structural validity.** `assayDropProofModel` is out of
  `REQUIRED_SEAL_FIELDS`, and an absent, ineligible, or wrongly typed value now yields
  `structurally-valid-not-credited` instead of `malformed`. Withholding credit is a verdict this
  consumer's own policy is entitled to reach, because it does read the member; the record stays
  verifiable for a consumer that does not read it at all. That split is the whole point of keeping
  the three outcomes distinguishable.
- **The producer's full payload is now describable.** `assayDropProofBasis` and `assayDropChannels`
  are known-optional to the checker rather than unlisted, so the twenty members a real run emits no
  longer read as two members the contract has never heard of. The producer keeps emitting them
  unconditionally, and `assayDropProofBasis` — the `checked`-versus-`declared` member — is the one
  worth not losing.
- **The rule has a test.** `aee_landlock_seal_fixture.py --producer-vocabulary-test` mutates the
  positive fixture and asserts the outcome. A phase is one word passed to `add`, invisible to the
  reason-code, rule-coverage, and required-field tests, all of which stayed green for two slices
  while this was wrong.

The counts that follow from it: `REQUIRED_SEAL_FIELDS` names **six** of the ten producer members,
the checker's known-optional set names the other four, and the checker's positive payload still
carries **eight** — the drop-proof basis and channels are permitted rather than fixtured, since the
Rust producer's key-set test is what pins their emission.

**The six siblings, decided 2026-08-07.** The passage above bounded itself to `assayDropProofModel`
and parked six sibling producer members — `assaySourceSchema`, `assaySealScope`,
`assayCollectionPath`, `assayAttackRowAttributionSource`, `assayNonClaims`, `assaySealedAt` — as six
contract questions rather than one edit. They are decided here, and all six move.

The reason they were parked does not survive being written out. It was that the normative paragraph
in in-toto/attestation#570 governs members whose values a reader might rank, and these carry
identities, instants and paths instead. That is true of the upstream paragraph and beside the point
for us: the prefix rule below is not scoped to rankable members. It says *fields beginning with
`assay`*, all of them, and has said so since this ADR was written. So six local decisions were being
held against an upstream sentence that could only ever be narrower than the rule we already had. The
question left open at in-toto/attestation#570 — whether a producer member is inert whether or not
its values rank — is still unanswered upstream as of this decision, and is not load-bearing here. An
answer would confirm the local rule, not extend it.

What was worth checking per member was never whether the rule reaches them. It was whether moving
each one gives up something the structural phase was buying.

- **`assaySourceSchema`** (`payload-source-schema-invalid`). A non-empty-string gate on a member the
  table above marks non-normative, and no other rule reads the value. Nothing attached, nothing lost.
- **`assaySealScope`** (`seal-scope-missing`, `seal-scope-mismatch`). #2014 is why this rule exists,
  and reading it settles the question rather than blocking it. The harm #2014 names is that the
  checker "credits it as attested substrate evidence", and the rule it holds up as the model to copy
  is `key-scope-collection-path-mismatch`, which is a not-credited rule. Withholding credit is the
  whole of what was asked for. A seal naming `filesystem_write_all` is still refused; it is refused
  as one this consumer will not credit rather than as a record no consumer can parse.
- **`assayCollectionPath`** (`payload-collection-path-mismatch`). This member was already being read
  at both phases in one file. `key-scope-collection-path-mismatch` withholds credit and
  `payload-collection-path-mismatch` voided the record, and the key-scope side is the one this ADR
  specifies itself: a credited signature requires that "the key's trusted scope includes the
  payload's `assayCollectionPath`", and a key outside scope "is structurally valid but not credited".
  #2106 reached the same place from the producer side — a path types a vantage, and must not rank one
  above another.
- **`assayAttackRowAttributionSource`** (`payload-attribution-source-unknown`, and
  `substrate-runner-observed-attacks-mismatch`). Two rules, and the second is why this member was the
  worst of the six rather than one more of them. It is not only gated against a closed set; its value
  *selects how strictly an AEE member is checked*. Under the attack-attribution rules below, equality
  between `aeeObservedAttacks` and the caught row attack IDs is required when the value is
  `substrate-runner` and not when it is `assembly-plane`. Changing the positive fixture from one of
  its two legal values to the other, with no other edit, turned a credited record into a malformed
  one. Both rules move. The baseline — every named attack must be supported by a caught row — reads
  only AEE members and stays structural; only the tightening is Assay policy, and only the tightening
  withholds credit.
- **`assayNonClaims`** (`payload-non-claims-incomplete`). The prefix paragraph below already rules on
  this member by name: it "is only producer vocabulary and does not weaken required AEE checks". A
  rule that lets an incomplete list void the record does the converse, letting producer vocabulary
  strengthen a required check into a rejection every consumer must honour. The checker's own comment
  asks that a subset be "a rejection rather than a warning", and it still is — not-credited is a
  rejection carrying a reason code, not a warning.
- **`assaySealedAt`** (`seal-instant-invalid`). The one with a real entanglement, and so the one to
  check rather than assume. The parsed instant is the input to `key-outside-validity-window`, and an
  unparsable value makes that check skip its record — so the structural finding was load-bearing for
  a bad instant not bypassing key expiry. It survives the move because the finding it raises is
  itself a not-credited finding: a seal whose instant will not parse cannot reach `credited` by any
  path, whether or not the window check ran. The window is not weakened. The record merely stops
  being called unreadable *by every consumer* on the strength of an Assay member.

Two things deliberately unchanged. The six stay in `REQUIRED_SEAL_FIELDS`, because that list states
what the Assay producer contract obliges a seal to carry, and the test over it asserts that absence
is not credited rather than that absence is malformed — so "required" reads as required-for-credit
for a producer member, which is the strongest thing the prefix rule permits it to mean. And
`assayObservedLabels`, `assayDropProofBasis` and `assayDropChannels` needed no decision at all: no
rule reads them, and nothing can demote what nothing consults.

Two checks were added with the move, because the existing ones could not see it.
`--producer-vocabulary-test` now also mutates a member to a *legal* alternate value, which is the
only mutation that reaches the selector case above — absent, ineligible and ill-typed are all
values `substrate-runner` is not. And it reads `validate` back, requiring every `assay`-prefixed member the
function consults to be one this file has decided, so the next producer member to acquire a rule
cannot acquire a phase by default. The generic ineligible value is now `""` rather than a borrowed
one, since `assaySourceSchema` accepts any non-empty string and would have passed a plausible
wrong value straight through.

Where this leaves the prefix rule: **seven** of the ten producer members are read by some rule in the
checker, all seven are enforced inert by test, and the remaining three are inert by having no rule.
Every rule that can still return `malformed` reads an `aee*` member, the envelope, or the predicate
structure.

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
