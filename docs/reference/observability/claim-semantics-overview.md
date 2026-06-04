# Claim Semantics Overview

> **Status:** orientation and composition reference. This page does not
> define a new schema. It explains how three contracts that already ship
> compose into one per-dimension claim-cell decision:
> [`claim-classes-v0`](claim-classes-v0.md) (the cell vocabulary),
> [`../runner/fidelity-verdict-v0`](../runner/fidelity-verdict-v0.md)
> (the capture-health gate), and
> [`../runner/coverage-descriptor-v0`](../runner/coverage-descriptor-v0.md)
> (the per-dimension coverage gate).

## Why a composition page

Each of the three contracts answers a different, narrow question, and a
single claim cell only becomes honest when all three are read together:

```text
claim-classes-v0       What kind of claim is this, and on what basis?
fidelity-verdict-v0    Was the capture healthy enough to interpret at all?
coverage-descriptor-v0 For this effect dimension, what could the method miss?
```

Read alone, any one of them can over- or under-state a result. A clean
fidelity verdict does not make an absence claim safe if the capture
method has a documented blind spot. A coverage descriptor with no blind
spots does not make a positive claim strong if the ring buffer dropped
events. The cell vocabulary has no opinion about either gate on its own.
This page fixes the order in which they apply so the same archive always
produces the same cell.

## The two-axis cell, restated

A claim cell carries two independent axes from
[`claim-classes-v0`](claim-classes-v0.md): `claim_strength`
(`strong` / `partial` / `weak` / `absent`) and `claim_basis`
(`reported` / `measured` / `derived` / `inferred`). Strength and basis
are independent: a measured positive can be `strong` inside its boundary
while an exhaustive set over the same dimension is only `weak`. The two
gates below decide where on those axes a given effect dimension lands.

## Gate one: capture health (fidelity verdict)

The first gate is run-wide and dimension-independent. It reads one
`assay.runner.observation_health.v0` record and produces a
`fidelity_verdict.v0`:

- `clean` — kernel capture present, no ring-buffer drops, cgroup
  correlation clean. Measured positive and bounded-negative claims may be
  interpreted, subject to gate two.
- `clipped` — capture ran but event loss is known. Measured positive
  claims degrade; bounded-negative claims are blocked regardless of
  coverage.
- `correlation_partial`, `failed`, `not_applicable` — see
  [`fidelity-verdict-v0`](../runner/fidelity-verdict-v0.md) for the full
  claim gate.

Capture health gates the *strength* of a measured positive claim. A
positive observed under a clean verdict is `strong`; the same observation
under a clipped verdict is `partial`. Blind spots do not enter here — a
healthy capture of an `openat` event is a real event even though the
method cannot see io_uring.

## Gate two: per-dimension coverage (coverage descriptor)

The second gate is dimension-specific. For each effect dimension a
`coverage_descriptor.v0` names the capture method, the positive effect
classes it observes, its `known_blind_spots`, and a `completeness`
ceiling. The descriptor gate evaluates the requested claim kind:

| Claim kind | Descriptor rule |
|---|---|
| `positive_existence` | Allowed when a descriptor is present; strength then comes from gate one. The shipped helper keys this on descriptor presence and does not yet validate that the claimed class appears in `observes` — the caller is responsible for selecting a descriptor whose `observes` covers the effect class. |
| `exhaustive_set` | Degraded to `weak` whenever the descriptor declares any blind spot or `completeness` is not `full`. |
| `bounded_negative` | Blocked whenever the descriptor declares any blind spot or `completeness` is not `full`. |

A missing or schema-mismatched descriptor is not the same as a present
one: the helper blocks coverage-aware claims outright when no valid
descriptor is supplied, rather than treating absence as permission.

The seed descriptors describe today's capture ceiling: filesystem capture
is `open_syscall_only` (io_uring and mmap-backed writes are blind spots),
network capture is `connect_only` (QUIC/datagram and io_uring are blind
spots), process capture is `exec_only` (fork/clone gaps). None of them
support complete claims yet, so every exhaustive set degrades and every
bounded-negative claim blocks under the current seeds. That is the point:
the gate must not silently upgrade a claim the method cannot back.

## Composition order

The two gates compose, they do not vote. The decision for one
`(dimension, claim_kind)` pair is:

```text
1. Capture health (gate one) decides whether measured interpretation is
   allowed at all, and the strength of a positive claim.
     clean   -> positive is strong
     clipped -> positive is partial; bounded-negative blocked
     other   -> see fidelity verdict claim gate
2. Coverage (gate two) decides the ceiling for exhaustive and
   bounded-negative claims for that dimension.
     blind spots or completeness != full
       -> exhaustive set degrades to weak
       -> bounded-negative blocks
3. The resulting cell records claim_strength + claim_basis, with the
   gate reason in notes; a blocked negative is not emitted as an
   allowed cell.
```

A bounded-negative claim is allowed only when *both* gates agree: capture
is clean and coverage is complete with no blind spots. Under the current
seeds the second condition never holds, so absence is never read as
safety. This is the load-bearing invariant: a zero count is the absence
of an observation, not proof that the effect did not occur.

## Worked example

The runnable example at
[`../../../examples/coverage-aware-side-effect/README.md`](../../../examples/coverage-aware-side-effect/README.md)
applies exactly this composition to a frozen Runner archive fixture. For
a clean capture with filesystem opens and a connect-only network
endpoint it emits:

- `measured_filesystem_effect` — `strong` / `measured` (clean capture,
  observed positive; the raw observation is measured).
- `exhaustive_filesystem_set` — `weak` / `derived`, with a note naming
  the gating rule and the `open_syscall_only` ceiling. The exhaustive
  reading is computed by applying the gate, so its basis is `derived`,
  not `measured`.
- `measured_network_effect` — `strong` / `measured`.
- `exhaustive_network_set` — `weak` / `derived`, naming the gating rule
  and the `connect_only` ceiling.
- `no_unexpected_filesystem_effect` and `no_unexpected_network_effect` —
  **blocked**, recorded in `blocked_claims` rather than emitted as cells.

Swap the clipped fixture and the positive cells move from `strong` to
`partial` without any other change, because gate one alone moved. The
blocked negatives stay blocked, because gate two never permitted them.

## Non-claims

- This page does not define a new schema, archive member, CLI output, or
  Trust Basis claim. It composes contracts that already ship.
- The composition does not close any blind spot a descriptor names; it
  only refuses to interpret past it.
- A `strong` measured positive is strong only inside the run's declared
  cgroup boundary; it does not prove intent or a complete view of effects.
- The example mirrors the canonical Rust gate in
  `crates/assay-runner-schema/src/coverage.rs`; the Rust helper remains
  the source of truth if the two ever diverge.
