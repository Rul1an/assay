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

An absent or null optional `extent` means **not stated** and follows legacy field verification.
Neither permits an extent-aware claim. New extent producers emit an object, never null. A
malformed non-null extension is refused. Its two required members are:

| Member | Value and meaning |
| --- | --- |
| `retained_events_by_type` | A sorted map from exact event-type strings to recomputed counts of every retained event. Only encountered types appear; absence is not synthesized as a zero-valued entry. |
| `observed` | A required object with an explicit `basis`, as described below. It carries producer assertions from a recognized summary, or explicitly says they are not stated. |

`observed` has two forms:

- `basis: "not_stated"`: the producer emits only `basis`. No recognized count summary exists.
  Known `source_type` or `counts` fields contradicting this basis are refused, rather than silently
  treated as checked information.
- `basis: "producer_reported"`: `source_type` and `counts` are required. `source_type` names the
  selected recognized summary event type. `counts` contains the fields that source schema states,
  using the mappings below. Null required structures, missing or unknown basis values, and
  malformed required counts are refused.

Unknown members of known structured objects remain ignored, consistent with ADR-044. Histogram
keys are data, not schema extensions: every entry participates in comparison. Every present known
field must agree with derivation from the verified artifact. A disagreement names a static field
path without echoing attacker-provided values. Optional parent presence is checked separately from
the required tagged structures inside it.

The claim boundary belongs in the specification's Parsing Rules, not an emitted `support_ceiling`
field. No separate `observed_source` field is emitted; the source belongs to the producer-reported
form. A reader that has only the Statement and does not consult its specification has no signed field naming the
claim boundary. Any explanation of that boundary must distinguish specification interpretation
from signed payload data.

An older reader can accept a signed statement while ignoring this extension. Such acceptance
means only that its known fields were checked. Admission that requires checked extent must require
a reader that checks it; absent, null, or ignored extent is not an affirmative extent claim. The
explicit basis avoids overloading a missing observation dimension; it does not assert that null
or an empty object mathematically means zero.

### 2. Recognize and rank exactly two summary schemas

Recognition in revision 1.1 uses this closed ordered list, highest priority first:

| Summary type | `files` | `network` | `processes` | `sandbox_degradations` |
| --- | --- | --- | --- | --- |
| `assay.profile.finished` | `files_count` | `network_count` | `processes_count` | `sandbox_degradation_count` |
| `assay.sandbox.summary` | `fs_count` | omitted | `exec_count` | `degradation_count` |

Selection order is deterministic, not a ranking of trust or measurement quality. Validate every
recognized summary, including a lower-ranked one that will not be selected. Repeated instances of
the same recognized type are refused, whether identical or conflicting. At most one of each type
may exist. When both valid types exist, select `assay.profile.finished` regardless of input order.
No recognized summary yields `observed: {"basis": "not_stated"}`.

All listed source count fields are required and must pass the shared numeric validation below.
The sandbox summary states no network count, so its generated `counts` omits `network`; omission
means not stated, never zero. An attested sandbox `network` count is a known-field disagreement,
not silently checked data. Required counts cannot be null, and producers emit no null inside
extent. Program-entry counts remain program-entry counts, not sums of per-program hits or syscall
counts. Profile counts concern aggregated profile entries, not independently measured activity of
one run.

Summary fields are producer assertions read from verified generic event payloads. Comparing them
with the artifact does not independently establish their truth, host activity, or observation
completeness. A recognized event type is not authentication of its producer. The existing typed
profile-summary decoder does not match all actual producer keys and is not the derivation source.

`assay.coding_agent.evidence_pack.v0` remains ordinary retained histogram data, never a count-summary
candidate. A normal sandbox bundle containing that pack and `assay.sandbox.summary` therefore has
one recognized summary. Extending the recognition list requires a documented revision and
compatibility assessment.

Malformed required counts and repeated recognized source types are refused only when deriving an
extent-aware statement or verifying a present non-null extent. The derivation never picks the
first or last row, sums summaries, or hides malformed data by turning it into not stated. Ordinary
bundle verification and absent/null-extent attestation verification retain legacy acceptance.

### 3. Use one exact integer domain and bound aggregation

Every count, including histogram values, is an integer in the inclusive domain
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
and discard only the new result projection: a recognized present non-null extent is still checked.

Signature verification, bundle verification, raw archive matching, and extent derivation are not
copied into a second consumer. Signature-only verification remains artifact-unmatched. The
extension creates no trust root, transparency log, completeness guarantee, policy-correctness
judgment, or provider-outcome verification.

## Required implementation evidence

The following are pending obligations, not results reported by this ADR:

- Behavioral cases must distinguish recomputed retained counts from producer-reported summary
  entry counts; preserve absent/null optional-parent equivalence; and cover unknown event types
  with `basis: "not_stated"`, sandbox network omission, aggregated profile entries, and
  program-entry rather than hit-count semantics. Inspect actual output bytes for no emitted null
  inside extent.
- False extent values must be signed after alteration so they reach named field comparison, not
  merely fail signature verification. Histogram, basis, source type, and count mismatches need
  their own refusal witnesses. Missing or unknown basis, null required structures, contradictory
  known fields under not_stated, and an attested sandbox network count must be covered.
- Exercise both input orders for the two valid recognized types, malformed lower-ranked input,
  repeated instances of each recognized type, and the normal sandbox summary plus coding-agent
  pack. Selection must not hide a malformed lower-ranked summary. The ordinary and absent/null
  paths must retain their acceptance controls.
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
  maximum-plus-one refusals before insertion, with ordinary and absent/null-extent paths preserved.
  Generated Statement bytes must satisfy the consumer's strict parser.
- Removing effective extent comparison, histogram accumulation, observed derivation, repeated-summary
  refusal, source selection, numeric ceiling, or aggregate bounds must defeat the corresponding
  behavioral test.
  A no-op control must pass. Successful setup or a failed signature is not a substitute for the
  intended refusal assertion.

## Consequences and implementation state

Documentation of the existing v1.0 predicate can be served independently of this extension.
Deployment still needs a fetch of the actual predicate Type URI; a local site build is not proof
that it resolves publicly. Implementation and publication evidence remain separate.

The accepted extension adds derivable detail without changing what an artifact signature means.
Producers may continue emitting v1.0, and absent or null optional extent remains not stated. The
new behavior needs the implementation and verification evidence above before the specification
describes it as current.
No CLI opt-in or extent-aware consumer is claimed by this documentation change.
