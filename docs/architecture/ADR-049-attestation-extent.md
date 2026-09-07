# ADR-049: Optional artifact-derived attestation extent

- Status: Accepted; implementation pending
- Date: 2026-09-07
- Applies: [ADR-044 decision 3](ADR-044-attestation-subject-is-the-artifact.md#decision)
- Supersedes: none
- Documentation: [#2832](https://github.com/Rul1an/assay/issues/2832)
- Library implementation: [#2833](https://github.com/Rul1an/assay/issues/2833)

This is an accepted technical decision, not a statement that the extension is implemented. At
source baseline `3a36085c90cef1b6c06170716a0351b6423e084d`, the producer emits the three-field
[v1.0 predicate](../attestation/evidence-bundle/v1.md). That specification continues to describe
current behavior. This ADR defines the obligations for an optional minor extension; it does not
change the wire format by being published.

## Context

The existing predicate states the semantic run root, event count, run identity, producer metadata,
and time window. Artifact verification checks these against the verified archive. The total event
count alone does not describe which event types the archive retained. A summary event may also
state counts of entries that were not exported individually. Those are different facts.

The run root covers the ordered content-hash inputs of retained events. Changing those inputs can
change the root; changing an export detail option without changing those inputs does not guarantee
a different root. Neither the option name nor the root establishes how much of a run was observed.

ADR-044 decision 3 already requires derivable predicate fields, ignores unknown fields within a
known major, and refuses unknown majors. This decision applies that rule to an optional extension;
it does not reopen the archive-subject boundary or the meanings of existing fields. Unknown-field
acceptance by an older reader is not verification of that field's contents.

There are two additional compatibility constraints. Published Rust result and predicate structs
have public fields, so adding even an optional field can break downstream literals and exhaustive
destructures. Also, a fixed object shape is not a fixed byte size: existing metadata strings can
grow, and a histogram adds variable keys and cardinality. Both constraints apply before claiming
an additive extension is compatible.

## Decision

### 1. Keep the major contract and distinguish absence from a checked statement

The extension is an optional top-level `predicate.extent` object under the same v1 Type URI and
`schema_version: 1`. Its parsing rules belong in specification revision 1.1 when implemented;
revision 1.1 is not the current specification published with this ADR. No existing field changes
meaning, and no `specVersion` payload field is introduced.

An absent `extent` means **not stated** and follows the v1.0 path. An explicit null parent is
malformed when recognized. A present extension has these required members:

| Member | Value and meaning |
| --- | --- |
| `retained_events_by_type` | A sorted map from exact event-type strings to counts of every retained event. Only encountered types appear; absence is not synthesized as a zero-valued entry. |
| `observed` | Null if there is no recognized summary; otherwise an object with required `files`, `network`, `processes`, and `sandbox_degradations` members. Each member is a count or null. Null means the selected summary does not state that dimension, never zero. |
| `observed_source` | Null exactly when `observed` is null; otherwise the exact recognized summary event-type string. This names the source of the count assertion, not an authenticated producer identity. |
| `support_ceiling` | Exactly the string `artifact_bound`, checked by equality. It names the existing attestation boundary and grants no additional support. |

Unknown members of known structured objects remain ignored, consistent with ADR-044. Histogram
keys are data, not schema extensions: every entry participates in comparison. Every present known
field must agree with derivation from the verified artifact. A disagreement names a static field
path without echoing attacker-provided values. Parent presence is checked separately from nested
null values.

An older reader can accept a signed statement while ignoring this extension. Such acceptance
means only that its known fields were checked. Admission that requires checked extent must require
a reader that checks it; neither absent nor ignored extent is an affirmative extent claim.

### 2. Recognize exactly two summary schemas

Recognition in revision 1.1 is limited to the following event types and mappings:

| Summary type | `files` | `network` | `processes` | `sandbox_degradations` |
| --- | --- | --- | --- | --- |
| `assay.profile.finished` | `files_count` | `network_count` | `processes_count` | `sandbox_degradation_count` |
| `assay.sandbox.summary` | `fs_count` | null | `exec_count` | `degradation_count` |

All listed source count fields are required and must pass the shared numeric validation below.
The sandbox summary states no network count. Program-entry counts remain program-entry counts,
not sums of per-program hits or syscall counts. More generally, these are assertions carried in
retained summary events; verifying them against the artifact does not independently measure host
activity or prove complete observations.

No recognized summary yields `observed: null` and `observed_source: null`. More than one recognized
summary is a refusal, including identical repeats, conflicting repeats, and a mixture of both
families. The derivation never picks the first or last, sums summaries, or hides malformed data by
turning it into null. A recognized summary with missing or malformed required counts is refused.
Extending the recognition list requires a documented revision and compatibility assessment.

These summary refusals apply only when deriving an extent-aware statement or verifying a present
extent. Ordinary bundle verification and absent-extent attestation verification retain their
existing accepted input set.

### 3. Use one exact integer domain and bound aggregation

Every non-null count, including histogram values, is an integer in the inclusive domain
`0..9007199254740991` (`0..2^53-1`), represented internally as `u64`. This is an interoperability
and representation ceiling, not a limit on host resources or a policy about observed activity.
The current JCS serialization path uses IEEE 754 number representation; unrestricted `u64` values
can round when serialized. The shared ceiling prevents that loss for accepted counts.

One definition and one conversion/validation function serve both summary families and histogram
counts. Negative values, fractional values, strings, null in required source count fields, and
values above the ceiling are refused before JCS serialization. Count addition and byte arithmetic
use checked operations.

The distinct histogram-key ceiling reads the strict parser's existing `MAX_KEYS_PER_OBJECT`
constant, currently 10,000; it is not maintained as a second literal. The collector also applies
one named aggregate decoded key-byte ceiling of 1 MiB, measured over UTF-8 key bytes. Both bounds
are checked before cloning or inserting a new key. They complement the existing bundle limits
and strict parser limits. Overflow or a resource refusal cannot silently truncate the histogram
or turn a requested extent into absence.

The actual JCS-canonicalized bytes of an extent-aware generated Statement must pass the same strict
JSON parser as the consumer. Passing a pre-serialization object check alone does not establish
producer/reader closure or numeric preservation. Existing `VerifyLimits` still apply before the
archive hash, as required by the canonical attestation verifier.

### 4. Preserve public Rust shapes and share the canonical verification path

The layouts and signatures of `EvidenceBundlePredicate`, `AttestationVerified`, `VerifyResult`,
and `VerifyLimits` remain unchanged. New public extent types and APIs are additive; their design
uses non-exhaustive structs or private fields with getters to avoid repeating the existing
constructor-compatibility constraint.

One accumulator derives extent during the canonical verified event pass, using an internal
`VerifiedBundle` sidecar only when extent is requested. Ordinary mode neither allocates a
histogram nor enforces summary-specific semantics. Both modes share one verification loop.

Additive producer APIs are `statement_for_bundle_with_extent` and
`statement_for_bundle_with_extent_and_limits`. Existing `statement_for_bundle` and
`statement_for_bundle_with_limits` remain v1.0 producers until a caller explicitly opts in.
Additive `verify_attestation_for_bundle_with_extent` and
`verify_attestation_for_bundle_with_extent_and_limits` APIs expose the original verified result
plus optional checked extent. Existing verification APIs share the canonical internal verifier
and discard only the new result projection: a recognized present extent is still checked.

Signature verification, bundle verification, raw archive matching, and extent derivation are not
copied into a second consumer. Signature-only verification remains artifact-unmatched. The
extension creates no trust root, transparency log, completeness guarantee, policy-correctness
judgment, or provider-outcome verification.

## Required implementation evidence

The following are pending obligations, not results reported by this ADR:

- Behavioral cases must distinguish retained type counts from summary entry counts, preserve
  absent-extent acceptance, exercise unknown event types with no summary, and cover sandbox
  network null and program-entry semantics.
- False extent values must be signed after alteration so they reach named field comparison, not
  merely fail signature verification. Each histogram, observed, source, and ceiling mismatch needs
  its own refusal witness. Parent null, missing fields, invalid count types, repeated summaries,
  and both recognized families must be covered.
- Downstream-style literals and exhaustive destructures must continue compiling for the existing
  public structs. Existing unknown-field acceptance stays covered. An immutable old-reader
  control must distinguish ignoring new fields from checking them.
- Numeric boundaries must cover acceptance of zero and `2^53-1`, and named refusal of `2^53` and
  `2^53+1`. Assert preserved counts in the decoded, signed DSSE payload, not only in the struct
  before signing. Include an independently assembled, otherwise valid NDJSON archive that retains
  `2^53+1` exactly, with event-byte length/digest and semantic roots consistent. Establish that
  ordinary verification accepts it before testing extent refusal. A BundleWriter-only witness
  can round the number earlier and miss the public byte-input boundary.
- Histogram cardinality and aggregate key-byte boundaries need accepted maximum controls and
  maximum-plus-one refusals before insertion, with ordinary and absent-extent paths preserved.
  Generated Statement bytes must satisfy the consumer's strict parser.
- Removing effective extent comparison, histogram accumulation, observed derivation, repeated-summary
  refusal, numeric ceiling, or aggregate bounds must defeat the corresponding behavioral test.
  A no-op control must pass. Successful setup or a failed signature is not a substitute for the
  intended refusal assertion.

## Consequences and implementation state

Documentation of the existing v1.0 predicate can be served independently of this extension.
Deployment still needs a fetch of the actual predicate Type URI; a local site build is not proof
that it resolves publicly. Implementation and publication evidence remain separate.

The accepted extension adds derivable detail without changing what an artifact signature means.
Producers may continue emitting v1.0, and absence remains not stated. The new behavior needs the
implementation and verification evidence above before the specification describes it as current.
No CLI opt-in or extent-aware consumer is claimed by this documentation change.
