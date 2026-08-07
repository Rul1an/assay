# SKETCH: ADR-045 Landlock seal payload contract

Status: Implementation sketch
Date: 2026-08-05
Applies: ADR-045
Related: #1998, #2000

## Summary

This sketch defines the next narrow implementation contract after the ADR-045
Landlock seal fixture/checker slice in #2000: a small Landlock/TCP-connect
run-end seal payload type and field-level validation contract.

This is deliberately **not** production seal emission, production signing,
checker integration, or stable AEE export. It is a payload contract sketch that
producer and checker work can converge on before production code is added.

## Research basis

The sketch follows five constraints from ADR-045 and current agent-security
practice:

1. **Evidence-first boundaries.** A seal payload must state only what the
   Landlock/TCP-connect substrate can honestly carry. It must not become a
   trust score, whole-action verdict, provider-outcome claim, or agent-safety
   claim.
2. **Attestation layering.** The seal payload is producer-owned payload
   vocabulary. A later AEE statement exporter may assemble it into an in-toto
   statement, but this payload type is not itself an in-toto predicate type.
3. **Verify-before-read.** Production consumers must verify the envelope
   signature and key scope over the exact signed bytes before crediting the
   payload as attested substrate evidence.
4. **Canonical bytes, not object-model luck.** Production signing must use a
   specified canonical UTF-8 JSON signing surface with duplicate object members
   rejected before signing or verification. The current fixture HMAC helper is
   non-normative.
5. **Operational auditability.** Agent systems fail in production when tool
   outputs, traces, and control boundaries are ambiguous. This sketch therefore
   names every carried claim and repeats the non-claims locally.

## Payload type

Use the following producer-owned media type for the first production-oriented
Landlock seal payload sketch:

```text
application/vnd.assay.landlock-run-end-seal.v0+json
```

Interpretation:

- `application/vnd.assay...+json` marks this as Assay producer vocabulary.
- `landlock-run-end-seal` scopes the payload to the Landlock/TCP-connect
  run-end seal primitive.
- `v0` marks the sketch as pre-stable and incompatible with any stable exporter
  guarantee.
- This media type is **not** an in-toto predicate type.
- This media type is **not** a DSSE envelope media type.
- Consumers must not infer stable AEE export support from its presence.

The current fixture harness uses
`application/vnd.assay.aee-landlock-seal.fixture.v0+json`; that fixture media
type remains experiment-only and must not be treated as production signing
semantics.

## Minimal payload shape

Illustrative JSON shape:

```jsonc
{
  "aeeKind": "sealed",
  "aeeVersion": "0.7",
  "aeeRunBinding": "<lowercase sha256 hex>",
  "aeeMethod": "intercepted",
  "aeePostureDigest": "<lowercase sha256 hex>",
  "aeeStillArmed": true,
  "aeeDropCount": 0,
  "aeeDropBound": 0,
  "assayDropProofModel": "synchronous-probe",
  "aeeObservedSet": "<lowercase sha256 hex>",
  "aeeObservedAttacks": [],
  "assayObservedLabels": ["connect_blocked"],
  "assayCollectionPath": "landlock-tcp-connect",
  "assaySealedAt": "2026-08-05T00:00:00Z",
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

## Field contract

| Field | Required | Type | Validation | Claim boundary |
|---|---:|---|---|---|
| `aeeKind` | yes | string | MUST equal `sealed`. | Identifies the covering record kind only. |
| `aeeVersion` | yes | string | MUST equal `0.7` for this sketch. | Shape-compatible with current AEE draft vocabulary; not a conformance claim. |
| `aeeRunBinding` | yes | string | Payload-only: MUST be lowercase SHA-256 hex. Post-assembly: MUST equal the shared run-binding derivation. | Binds this seal to the run inputs, not to a semantic evidence chain. |
| `aeeMethod` | yes | string | For first slice, MUST equal `intercepted`. | Does not imply provider side-effect verification. |
| `aeePostureDigest` | yes | string | Payload-only: MUST be lowercase SHA-256 hex. Post-assembly: MUST equal the assembled statement's `predicate.observationEnvironment.networkPosture.digest.sha256`. | Binds the carried posture descriptor, not the full run binding preimage. |
| `aeeStillArmed` | yes | boolean | MUST be `true` for a successful Landlock seal. Unknown or failed state is invalid. | Claims run-end still-armed state only under ADR-045 proof rules. |
| `aeeDropCount` | yes | integer | First slice MUST be `0`. | Counts observation drops/losses, not blocked policy events. |
| `aeeDropBound` | yes | integer | First slice MUST be `0`. | Bounds unobserved/lost observations under the named proof model only. |
| `assayDropProofModel` | yes | string | Producer MUST emit `synchronous-probe` or `counted-queue-zero` in the first slice. A consumer reading it MUST withhold credit for any other value and MUST NOT treat the record as structurally invalid, per ADR-045's producer-vocabulary resolution. | Identifies the proof model that makes zero-drop accounting creditable. |
| `aeeObservedSet` | yes | string | Payload-only: MUST be lowercase SHA-256 hex. Post-assembly: MUST recompute over emitted interception/examination record leaves. | Digest commitment; not a label array. |
| `aeeObservedAttacks` | yes | array of strings | Payload-only: MUST be an array whose every member is a string. Post-assembly: each named attack MUST be supported by a caught row unless validating standalone before statement assembly. | Lower-bound substrate attribution, not completeness. Empty is valid for pure Landlock assembly-plane attribution. |
| `assayObservedLabels` | no | array of strings | If present, MUST NOT substitute for `aeeObservedSet`. | Operator/debug vocabulary only. |
| `assayCollectionPath` | yes | string | First slice MUST equal `landlock-tcp-connect`. Credited-evidence validation also checks trusted key scope. | Prevents flattening all substrate observations into one undifferentiated source. |
| `assaySealedAt` | yes | string | MUST be an RFC 3339 UTC instant (`YYYY-MM-DDTHH:MM:SSZ`). | Names the run-end instant the seal commits to. Checked against the signing key's validity window; not a trusted timestamp and not an anchor. |
| `assaySourceSchema` | yes | string | MUST be a non-empty string. SHOULD be `assay.enforcement_health.v1`. | Names the Assay source vocabulary, not external AEE conformance. |
| `assaySealScope` | yes | string | First slice MUST equal `tcp_connect_landlock_port`. | Names the bounded enforcement scope. |
| `assayAttackRowAttributionSource` | yes | string | MUST be `assembly-plane` or `substrate-runner`. Equality with caught rows is required only for `substrate-runner`. | Must not upgrade assembly-plane attribution into substrate claim. |
| `assayNonClaims` | yes | array of strings | MUST include the payload-local minimum non-claims listed below unless superseded by a later ADR. | Payload-local producer non-claims; does not replace predicate-level `doesNotAssert`. |

## Payload-local non-claims

For this sketch, `assayNonClaims` MUST include at least these payload-local
non-claims:

- `does not prove complete run population`
- `does not prove agent safety`
- `does not prove provider side effects`
- `does not prove independent substrate operation`

The broader document-level non-claims remain normative for the sketch and PR
scope, but they are not all required as payload-local `assayNonClaims` entries.
Any future AEE statement exporter must still carry statement-level non-claims in
predicate-level `doesNotAssert` as required by ADR-045.

## Validation phases

This sketch separates three validation phases so the payload contract does not
pretend a payload-only validator has statement or trust-policy context.

### Payload-only validation

Payload-only validation can check only the sealed payload object and its local
field constraints:

1. The payload is a JSON object.
2. Every required field is present.
3. Every field has the expected primitive type.
4. Every digest-shaped value is lowercase SHA-256 hex.
5. `aeeKind` equals `sealed`.
6. `aeeVersion` equals `0.7` for this sketch.
7. `aeeMethod` equals `intercepted` for the first slice.
8. `aeeStillArmed` is true.
9. `aeeDropCount` and `aeeDropBound` are both zero.
10. `assayDropProofModel` is `synchronous-probe` or `counted-queue-zero`. This is
    the one item on this list that withholds credit rather than voiding the
    payload: it is Assay producer vocabulary, and ADR-045 records that such a
    member never alters AEE structural validity.
11. `assayCollectionPath` equals `landlock-tcp-connect` for the first slice.
12. `assaySealScope` equals `tcp_connect_landlock_port` for the first slice.
13. `assayAttackRowAttributionSource` is `assembly-plane` or
    `substrate-runner`.
14. `assayNonClaims` includes the payload-local minimum non-claims above.
15. `assaySourceSchema` is a non-empty string.
16. `aeeObservedAttacks` is an array whose every member is a string.
17. `assaySealedAt` is an RFC 3339 UTC instant.

### Assembled-statement validation

Assembled-statement validation has access to the carried statement around the
payload and can check cross-record invariants:

1. `aeeRunBinding` equals the shared run-binding derivation.
2. `aeePostureDigest` equals the assembled statement's
   `predicate.observationEnvironment.networkPosture.digest.sha256`.
3. `aeeObservedSet` recomputes over emitted interception/examination record
   leaves.
4. `aeeObservedAttacks` names only attacks supported by caught rows when the
   seal is checked after statement assembly.
5. `assayAttackRowAttributionSource = "substrate-runner"` requires
   `aeeObservedAttacks` to equal the sorted caught-row attack IDs.
6. Every substrate row that claims sealed coverage actually references a valid
   sealed record.

### Credited-evidence validation

Credited-evidence validation has access to signature verification material and
consumer trust policy. It decides whether a structurally valid signed record is
credited as attested substrate evidence:

1. The envelope's payload type is one the checker implements. A shape it does
   not implement is **rejected as unsupported**, never skipped: a
   "we did not check this" path that returns success is how an unverified record
   comes to read as verified.
2. The envelope signature verifies over the exact signed bytes, under the
   payload type the envelope declares.
3. The signing key is trusted by consumer policy.
4. The signing key role is `substrate-observation`.
5. The signing key's trusted scope includes the payload's
   `assayCollectionPath`.
6. The signing key's trusted scope is compatible with the statement's substrate
   descriptor.
7. The seal's `assaySealedAt` falls inside the signing key's trusted validity
   window. A checker with no notion of a window silently keeps crediting a
   retired key, and adding the check later is a breaking change to every fixture
   already written. This is not key management: the window is handed to the
   checker, and rotation, revocation, and distribution belong with the
   production signing primitive.
8. A fixture key or fixture signer is not present in a production path.

A valid signature from an untrusted or out-of-scope key is structurally valid as
an envelope fact, but it is not credited as attested substrate evidence. The
three outcomes a checker reports must stay distinguishable — `malformed`,
`structurally-valid-not-credited`, and `credited` — because ADR-043's rule that
integrity never upgrades meaning only exists if a consumer can tell the second
from the first.

## Canonicalization and signing boundary

Production signing is intentionally not selected in this sketch. Before a stable
exporter is exposed, a later implementation must define:

- envelope format;
- signature algorithm;
- production key role and key scope policy;
- payload type binding;
- canonical JSON algorithm;
- duplicate-member rejection rule;
- verify-before-read rule over exact signed bytes;
- resource ceilings before payload materialization.

For this sketch, the intended production direction remains:

- canonical JSON encoded as UTF-8;
- duplicate object members rejected before signing or verification;
- no ProtoJSON or generated binding output as the canonical signing surface;
- no fixture HMAC key accepted as production substrate evidence.

The fixture helper in `scripts/experiments/aee_landlock_seal_fixture.py` remains
useful for development semantics, but it does not define production signing.

## Relationship to in-toto/AEE

This sketch is AEE-compatible vocabulary, not stable AEE export.

A later exporter must still assemble an in-toto Statement whose subject follows
ADR-044: the subject identifies the executed artifact by artifact digest, not an
Assay semantic evidence chain. The Statement-level `predicateType` remains the
place where an AEE predicate type is declared. This payload type is merely an
Assay-produced sealed record that such an exporter may require.

Unknown `assay*` fields may be ignored by consumers that do not understand Assay
producer vocabulary. They must not weaken AEE structural checks.

## Non-claims

This sketch does not claim:

- production AEE support;
- stable AEE export;
- production signing semantics;
- complete run-population proof;
- independent substrate operation;
- agent safety;
- provider side-effect verification;
- policy-decision key authority over substrate observations;
- full AEE conformance checker coverage.

## Next implementation slice

The fixture/checker hardening this sketch called for landed in #2006:

1. Fixture policy decided — full on-disk positive, marker-only negative controls
   (`docs/experiments/aee-landlock-seal-fixtures-2026-08.md`).
2. Six key-scope negative controls added, each asserted on its own reason code:
   untrusted key, wrong key role, collection-path mismatch, substrate mismatch,
   validity window, unsupported envelope shape.
3. `batchRoot` removed. It carried no semantics and no check, and an undefined
   field crossing a signing boundary is more expensive than reintroducing it
   later with defined semantics.
4. Exporter commands stayed out of scope.

Producer-side Landlock seal integration should start only now that the checker
fails closed on malformed and uncredited seal fixtures.
