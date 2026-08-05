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
  "aeeObservedSet": "<lowercase sha256 hex>",
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

## Field contract

| Field | Required | Type | Validation | Claim boundary |
|---|---:|---|---|---|
| `aeeKind` | yes | string | MUST equal `sealed`. | Identifies the covering record kind only. |
| `aeeVersion` | yes | string | MUST equal `0.7` for this sketch. | Shape-compatible with current AEE draft vocabulary; not a conformance claim. |
| `aeeRunBinding` | yes | string | MUST be lowercase SHA-256 hex derived by the shared run-binding function. | Binds this seal to the run inputs, not to a semantic evidence chain. |
| `aeeMethod` | yes | string | For first slice, MUST equal `intercepted`. | Does not imply provider side-effect verification. |
| `aeePostureDigest` | yes | string | MUST equal `observationEnvironment.networkPosture.digest.sha256`. | Binds the carried posture descriptor, not the full run binding preimage. |
| `aeeStillArmed` | yes | boolean | MUST be `true` for a successful Landlock seal. Unknown or failed state is invalid. | Claims run-end still-armed state only under ADR-045 proof rules. |
| `aeeDropCount` | yes | integer | First slice MUST be `0`. | Counts observation drops/losses, not blocked policy events. |
| `aeeDropBound` | yes | integer | First slice MUST be `0`. | Bounds unobserved/lost observations under the named proof model only. |
| `aeeObservedSet` | yes | string | MUST be lowercase SHA-256 hex over emitted interception/examination record leaves. | Digest commitment; not a label array. |
| `aeeObservedAttacks` | yes | array of strings | Each named attack MUST be supported by a caught row unless validating standalone before statement assembly. | Lower-bound substrate attribution, not completeness. Empty is valid for pure Landlock assembly-plane attribution. |
| `assayObservedLabels` | no | array of strings | If present, MUST NOT substitute for `aeeObservedSet`. | Operator/debug vocabulary only. |
| `assayCollectionPath` | yes | string | First slice MUST equal `landlock-tcp-connect`. | Prevents flattening all substrate observations into one undifferentiated source. |
| `assaySourceSchema` | yes | string | First slice SHOULD be `assay.enforcement_health.v1`. | Names the Assay source vocabulary, not external AEE conformance. |
| `assaySealScope` | yes | string | First slice MUST equal `tcp_connect_landlock_port`. | Names the bounded enforcement scope. |
| `assayAttackRowAttributionSource` | yes | string | MUST be `assembly-plane` or `substrate-runner`. Equality with caught rows is required only for `substrate-runner`. | Must not upgrade assembly-plane attribution into substrate claim. |
| `assayNonClaims` | yes | array of strings | MUST include the non-claims listed in this sketch unless superseded by a later ADR. | Payload-local producer non-claims; does not replace predicate-level `doesNotAssert`. |

## Structural validation sketch

A checker for this payload shape must fail closed when any of the following is
true:

1. The payload is not a JSON object.
2. Any required field is absent.
3. Any digest field is not lowercase SHA-256 hex.
4. `aeeKind` is not `sealed`.
5. `aeeVersion` is not `0.7` for this sketch.
6. `aeeRunBinding` does not equal the shared run-binding derivation.
7. `aeePostureDigest` does not equal the carried
   `observationEnvironment.networkPosture.digest.sha256`.
8. `aeeStillArmed` is not true.
9. `aeeDropCount` or `aeeDropBound` is non-zero for the first Landlock slice.
10. The named drop-accounting proof model is absent or not eligible under
    ADR-045.
11. `aeeObservedSet` does not recompute over emitted interception/examination
    record leaves.
12. `aeeObservedAttacks` names an attack unsupported by a caught row when the
    seal is checked after statement assembly.
13. `assayAttackRowAttributionSource = "substrate-runner"` and
    `aeeObservedAttacks` is not equal to the sorted caught-row attack IDs.
14. `assayCollectionPath` is not within the trusted key scope credited by
    consumer policy.
15. The signer key role is not `substrate-observation` when the record is being
    credited as attested substrate evidence.
16. A fixture key or fixture signer appears in a production path.

A valid signature from an untrusted or out-of-scope key is structurally valid as
an envelope fact, but it is not credited as attested substrate evidence.

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

After this sketch lands, the next implementation work should be a separate PR
that hardens the fixture/checker boundary before producer code:

1. Decide whether committed fixtures are marker-only or full on-disk artifacts.
2. Add key-scope negative controls for untrusted key, wrong key role, and wrong
   collection path.
3. Remove or validate `batchRoot` in the fixture slice.
4. Keep exporter commands out of scope.

Producer-side Landlock seal integration should start only after the checker can
fail closed on malformed or missing production-oriented seal fixtures.
