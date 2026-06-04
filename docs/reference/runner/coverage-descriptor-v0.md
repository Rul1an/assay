# Runner Coverage Descriptor v0

> **Status:** internal helper contract. This page defines the
> `assay.runner.coverage_descriptor.v0` vocabulary and the first
> descriptor-driven claim-kind gate. It does not add a Runner archive
> member, CLI output, Trust Basis claim, or stable report schema.

## Purpose

Runner observation health answers whether the capture was healthy. A
coverage descriptor answers a different question:

```text
For this effect dimension, what did this capture method observe, and what can it miss?
```

That distinction matters because an observed positive event can remain
useful while an exhaustive or negative claim is unsafe. For example, a
file open observed by an `openat` tracepoint is still a measured positive
event, but `openat`-only capture does not prove that no other file
activity happened.

## Schema String

```text
assay.runner.coverage_descriptor.v0
```

The current Rust helper lives in `assay-runner-schema`. The schema string
is an internal helper/contract label at this stage; no JSON Schema
sidecar or archive member is frozen yet.

## Descriptor Shape

```json
{
  "schema": "assay.runner.coverage_descriptor.v0",
  "dimension": "filesystem",
  "method": "open/openat/openat2 tracepoints",
  "observes": ["path opens through syscall tracepoints"],
  "known_blind_spots": [
    "io_uring file operations may bypass syscall tracepoints",
    "mmap-backed writes are not path-open observations"
  ],
  "completeness": "open_syscall_only"
}
```

Fields:

| Field | Meaning |
|---|---|
| `dimension` | Effect dimension: `filesystem`, `network`, or `process`. |
| `method` | Capture method being described. |
| `observes` | Positive effect classes this method can report. |
| `known_blind_spots` | Documented ways this method may miss effects. |
| `completeness` | The claim ceiling for exhaustive and negative claims. |

`known_blind_spots` is data, not prose decoration. Downstream claim gates
must consult it before interpreting absence or exhaustiveness.

## Seed Descriptors

| Dimension | Helper | Completeness | Initial blind spots |
|---|---|---|---|
| filesystem | `filesystem_open_syscall_only()` | `open_syscall_only` | io_uring file operations; mmap-backed writes |
| network | `network_connect_only()` | `connect_only` | QUIC/datagram peer changes after connect; io_uring network operations |
| network | `network_datagram_peer_observed()` | `datagram_peer_observed` | connected datagram sends without explicit sockaddr; io_uring network operations |
| network | `network_connect_and_datagram_peer_observed()` | `connect_and_datagram_peer_observed` | io_uring network operations |
| process | `process_exec_only()` | `exec_only` | fork/clone gaps that affect process-tree exhaustiveness |

The seed descriptors intentionally describe the current capture ceiling.
Future capture improvements can narrow or remove blind spots by changing
descriptor data; until then, the gate must not silently upgrade claims.
`network_for_protocol_coverage(status)` maps
`observation_health.network_protocol_coverage` into the matching network
descriptor when the run reported `connect_only`, `datagram_peer_observed`,
or `connect_and_datagram_peer_observed`. `unknown` and `absent` return no
descriptor, so coverage-aware network claims remain blocked rather than
silently assuming a method.

## Claim Kinds

The descriptor gate evaluates the requested claim kind:

| Claim kind | Example | Descriptor rule |
|---|---|---|
| `positive_existence` | "This path open happened." | Allowed for the caller-scoped effect class when a descriptor is present. |
| `exhaustive_set` | "These are all paths or peers." | Degraded when any blind spots are declared. |
| `bounded_negative` | "No unexpected egress happened." | Blocked when any blind spots are declared. |

The base gate (`claim_decision_for`) is intentionally conservative: it does
not inspect whether a particular claimed effect class appears in `observes`,
so callers that use it remain responsible for selecting a descriptor whose
`observes` covers the effect class they are interpreting.

An additive, effect-class-aware variant `claim_decision_for_effect(descriptor,
claim_kind, effect_class)` layers that check on top. For `positive_existence`
it first applies the same presence/schema/claim-kind gate and then, when the
base gate would allow the positive claim, additionally confirms the
`effect_class` is one the descriptor observes (a conservative case-insensitive
containment match against `observes`, also exposed as
`observes_effect_class`). A positive claim for a class outside `observes` is
downgraded to `degraded` (`coverage_descriptor_positive_class_not_observed`)
rather than blanket-allowed. `exhaustive_set` and `bounded_negative` are
unaffected, because they already gate on completeness and blind spots, which
are class-independent; the base `claim_decision_for` is left unchanged for
callers that scope the effect class themselves.

Relevance filtering of individual blind spots per claim remains a later step:
if a descriptor declares any blind spot, both variants still treat it as
relevant for exhaustive and bounded-negative claims.

M1 also treats `completeness` as load-bearing, not decorative. Exhaustive
and bounded-negative claims are allowed only when `completeness = full`
and the descriptor declares no blind spots. A descriptor with
`completeness = open_syscall_only`, `connect_only`, or `exec_only` still
degrades or blocks those claim kinds even if its blind spot text is
accidentally empty.

This composes with `fidelity_verdict.v0`. Fidelity gates capture health
such as drops and cgroup correlation. Coverage descriptors gate the
dimension-specific ceiling even when fidelity is otherwise `clean`.

## Derived Decisions

The helper emits a small `CoverageClaimDecision`:

```json
{
  "decision": "blocked",
  "rule": "coverage_descriptor_blocks_absence_claim",
  "reason": "filesystem blind spots can hide the requested absence: io_uring file operations may bypass syscall tracepoints; mmap-backed writes are not path-open observations"
}
```

Initial rules:

- Missing descriptor blocks coverage-aware side-effect claims.
- Schema mismatch blocks coverage-aware side-effect claims.
- Positive existence is `allowed` for a present descriptor, scoped by the
  caller to an effect class that descriptor observes.
- Exhaustive set is `allowed` only when `completeness = full` and the
  descriptor has no known blind spots; otherwise it is `degraded`.
- Bounded negative is `allowed` only when `completeness = full` and the
  descriptor has no known blind spots; otherwise it is `blocked`.

## Non-Claims

- The descriptor does not prove runtime safety.
- The descriptor does not close the blind spot it names.
- The descriptor does not replace `observation_health.v0`.
- The descriptor does not convert `connect_only` or datagram-aware network
  capture into an exact peer set.
- The descriptor does not make self-reported SDK or trace evidence
  measured.

## Wiring Status

The Rust helper remains an internal contract in `assay-runner-schema`.
Experiment surfaces may mirror it for sidecar annotations and enforcement, but
the Runner archive contract is unchanged. This helper still does not:

- add a Runner archive member;
- add CLI output;
- add a stable report schema;
- add Trust Basis claims;
- add capture enhancements for io_uring or fork/clone.

Any consumer that mirrors the gate should preserve the same ceiling: datagram
peer observations can strengthen positive network evidence, but they do not
permit exact peer-set or bounded-negative network claims while blind spots
remain declared.
