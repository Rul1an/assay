# ADR-045 Landlock seal fixture slice

Status: implementation-fixture slice for issue #1998, hardened in #2006.

This experiment is deliberately narrower than the AEE fixture spike in
`docs/experiments/aee-assay-spike-2026-08.md`. It does not add a production AEE
exporter and does not claim production AEE support. It pins the first checker
contract for the ADR-045 Landlock/TCP-connect run-end seal primitive.

## Files

- `scripts/experiments/aee_landlock_seal_fixture.py` emits and validates named
  ADR-045 Landlock seal fixtures.
- `scripts/experiments/fixtures/aee-landlock-seal/valid-landlock-seal.json`
  is the positive fixture, committed in full.
- `scripts/experiments/fixtures/aee-landlock-seal/negative-controls/*.json`
  are markers for the targeted invalid cases.

## Fixture policy

The positive fixture is a **full on-disk artifact**; the negative controls are
**markers**.

That split is not a formatting preference. The positive fixture is what a
producer gets checked against, and the bytes it commits to are the signing
surface, so emitter drift is signing-surface drift and has to be visible in
review. A negative control is defined by the single field it breaks, which its
marker already names in `case` and `rejectsWith`; committing a full body for each
would add nine near-identical artifacts that reviewers cannot diff usefully.

The checker reads the positive fixture back from disk and generates the negative
controls, so a hand-edit to a control cannot silently drift away from the check
it targets.

Two preconditions make this safe, and both hold:

- **Deterministic generation.** No wall clock, random IDs, or locale-dependent
  formatting. The seal instant `assaySealedAt` and the key validity window are
  fixed constants, precisely so a validity-window check does not make the drift
  check flaky.
- **Regeneration is explicit.** `--emit` writes; it never runs as a side effect
  of a check.

Before this slice, the emitter wrote a full body for every case while all ten
committed files were markers, so the drift check below could not have passed. It
had never been run.

## Smoke check

```sh
python3 scripts/experiments/aee_landlock_seal_fixture.py --emit
python3 scripts/experiments/aee_landlock_seal_fixture.py valid-landlock-seal
while read -r name reason; do
  python3 scripts/experiments/aee_landlock_seal_fixture.py "$name" \
    --expect-invalid --expect-reason "$reason"
done <<'PAIRS'
missing-seal substrate-row-missing-sealed-coverage
bad-run-binding run-binding-mismatch
not-still-armed seal-not-still-armed
bad-drop-accounting drop-accounting-nonzero
uncounted-channel-without-eligible-seal drop-proof-model-ineligible
bad-observed-set observed-set-mismatch
unsupported-observed-attack observed-attack-unsupported
substrate-runner-observed-attacks-mismatch substrate-runner-observed-attacks-mismatch
fixture-key-production-scope fixture-key-in-production-path
untrusted-signing-key untrusted-signing-key
wrong-key-role wrong-key-role
key-scope-collection-path-mismatch key-scope-collection-path-mismatch
key-scope-substrate-mismatch key-scope-substrate-mismatch
key-outside-validity-window key-outside-validity-window
unsupported-envelope-shape unsupported-envelope-shape
PAIRS
```

`--expect-reason` is the part that matters. A control asserted only on a non-zero
exit reports coverage it does not have, because it cannot distinguish "rejected
for the reason I built" from "rejected because I broke the JSON".

Case name and reason code are separate columns because they are separate things,
and this block used to pass the case name as the reason. Eight of the fifteen
cases then asserted a code no rule emits, so eight lines of a snippet published as
the way to check this slice had never been run. `--meta-test` is the maintained
form of the same property and is what a gate should call; this block is here to be
read.

Fixture drift:

```sh
python3 scripts/experiments/aee_landlock_seal_fixture.py --emit
git diff --exit-code -- scripts/experiments/fixtures/aee-landlock-seal/
```

Every control isolates its own reason:

```sh
python3 scripts/experiments/aee_landlock_seal_fixture.py --meta-test
```

The meta-test disables exactly the reason code a control targets and requires the
case to become credited. If it still fails, the control was failing for some
other reason and its coverage was imaginary. `--disable-check` exists for that
test and for nothing else.

Producer vocabulary never voids a record:

```sh
python3 scripts/experiments/aee_landlock_seal_fixture.py --producer-vocabulary-test
```

Each member in `PRODUCER_CREDIT_FIELDS` is removed, given an ineligible value, and
given the wrong type, and every mutation must land on
`structurally-valid-not-credited`. The three tests above are blind to this: a phase
is one word passed to `add`, so a rule can move between outcomes without changing a
reason code, a control, or a required field.

Two mutations are not of that shape. A member listed in
`PRODUCER_CREDIT_ALTERNATE_VALUES` is also set to one of its own *legal* values,
and that mutation asserts only that the record is not voided — a legal value may
well leave it credited. This is what catches a member whose permitted value
decides how strictly some other rule runs, which absent, ineligible and ill-typed
mutations cannot reach. The test then reads `validate` back and requires every
`assay`-prefixed member the function consults to appear in
`PRODUCER_CREDIT_FIELDS`, so a producer member that acquires a rule cannot acquire
a phase by default.

## Three outcomes

The checker distinguishes three outcomes, because ADR-043's rule that integrity
never upgrades meaning only exists if a consumer can tell them apart:

| Outcome | Meaning |
|---|---|
| `malformed` | Not structurally valid. |
| `structurally-valid-not-credited` | The record is well formed, and this consumer's policy withholds credit — an untrusted key, one out of scope, in the wrong role, outside its validity window, or an Assay producer member whose value this policy reads and does not accept. |
| `credited` | Structurally valid, and the key is trusted for this scope. |

If the middle outcome rendered identically to the first, the distinction would
exist only in prose.

## Contract pinned by this slice

Structural rejections (`malformed`):

- a substrate row has no sealed, arming, or interception coverage;
- a seal carries a bad run binding;
- `aeeStillArmed` is not true;
- `aeeObservedSet` does not recompute over emitted interception/examination record leaves;
- `aeeObservedAttacks` names attacks unsupported by caught rows;
- `aeeObservedAttacks` is not an array of strings;
- the envelope's payload type is one this checker does not implement — rejected
  as unsupported, never skipped, because a "we did not check this" path that
  returns success is how an unverified record comes to read as verified.

Every rule that can still return `malformed` reads an `aee*` member, the envelope,
or the predicate structure. That is the property, not a coincidence of the current
list.

Read means interprets. Mutating a producer member inside an interception record
does change `aeeObservedSet` and so can return `malformed`, because the observed
set is a digest over payload bytes and commits to them opaquely. That is not a
breach of the prefix rule: a consumer that ignores Assay vocabulary recomputes the
identical digest and reaches the identical verdict, so there is no disagreement
between consumers, which is the harm the rule names.

Trust rejections (`structurally-valid-not-credited`):

- the signing key is not in the consumer trust set;
- the key's role is not `substrate-observation`;
- the key's trusted scope excludes the payload's `assayCollectionPath`;
- the key's trusted scope names a different substrate than the statement;
- the seal instant falls outside the key's validity window;
- a fixture key is used in a production-like path;
- `assayDropProofModel` names no eligible proof model for the zero drop accounting,
  or is absent;
- `assaySourceSchema` is absent or not a non-empty string;
- `assaySealScope` is absent, or names a boundary other than the one this slice
  observes;
- the seal's `assayCollectionPath` is not the path this slice collects on;
- `assayAttackRowAttributionSource` is outside its closed set, or is
  `substrate-runner` while `aeeObservedAttacks` and the caught rows disagree;
- `assayNonClaims` omits one of the payload-local minimum non-claims;
- `assaySealedAt` is not an RFC 3339 UTC instant.
- `assayDropProofBasis` disagrees with the basis `assayDropProofModel` implies — a payload claiming
  `checked` over a model that verified nothing (#2093).
- `assayDropChannels` disagrees with the basis: a `checked` basis rests on readings and a `declared`
  one has none.
- `assayDropChannels` is not a list.

Everything from `assayDropProofModel` down is a policy verdict rather than a
structural one. Three of them used to be filed above; the rest were structural
in the checker without this document ever saying so, and `assaySealScope` and
`assayNonClaims` appear here for the first time. ADR-045 states that
`assay`-prefixed members are producer vocabulary and must not alter AEE structural
validity, and the checker was breaking its own rule: values only Assay defines
decided whether an AEE record was well formed. This consumer's policy does read
those members, so it may withhold credit; a consumer whose policy does not read
them must still see a verifiable seal. `assayDropProofModel` was corrected first
and committed to publicly in
[in-toto/attestation#570](https://github.com/in-toto/attestation/pull/570#issuecomment-5216879286);
the six siblings follow it, each decided on its own in ADR-045.

The attribution entry is two rules, and the second is the one worth knowing about.
`assayAttackRowAttributionSource` does not merely carry a gated value — its value
*selects* whether `aeeObservedAttacks` must equal the caught rows or may be a
lower bound. So flipping that one member between its two legal values used to move
the record between `credited` and `malformed`, which is the prefix rule's failure
mode in its purest form: two consumers disagreeing about whether a record is well
formed, because one of them reads Assay vocabulary and the other does not.

The validity window is not key management. This slice checks that a window handed
to the checker is honoured; rotation, revocation, and distribution belong with the
production signing primitive. A checker with no notion of a window silently keeps
crediting a retired key, and adding one later is a breaking change to every
fixture already written.

`batchRoot` was removed. It was present with no stated semantics and no check,
and carrying an undefined field through a signing boundary is more expensive than
reintroducing it later with defined semantics.

## Non-claims

This slice does not claim:

- production AEE support;
- stable AEE export;
- production signing semantics;
- consumer trust configuration or key distribution;
- complete run-population proof;
- independent substrate operation;
- agent safety;
- provider side-effect verification.
