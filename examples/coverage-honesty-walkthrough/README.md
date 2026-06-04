# Coverage honesty walkthrough (end-to-end)

This is a reading guide, not new code. It ties the existing coverage examples
into one story so you can follow a coverage claim from the kernel observation
that grounds it all the way to a gate that refuses to overstate it. Every stage
below is a real, runnable example already in this repository; this page just puts
them in order and explains how each one constrains the next.

The throughline: **a claim is never stronger than the observation that backs it,
and every stage degrades rather than inflates when evidence is missing.**

## The stages

### 1. Capture — what was actually observed
The Runner observes side effects at the kernel boundary (file opens, network
connects, datagram peers, process execs) without instrumenting the application.
That raw observation is the only thing the whole chain is allowed to treat as
ground truth. Everything downstream is a claim *about* this capture, bounded by
how complete the capture was.

### 2. Coverage descriptor — how complete the observation was
A coverage descriptor records the completeness of the observation for each effect
class (for example: were only `open`-family syscalls seen? was network capture
connect-only, or were datagram peers also observed?). This is the honesty
ceiling: it says what the capture can and cannot support, before any claim is
made. See the descriptor semantics in `crates/assay-runner-schema/src/coverage.rs`.

### 3. Annotation — turning a drift report into honest claim cells
When two runs are compared, the drift report says *what differs*. The coverage
annotation says *how strongly you may claim it*. Each drift dimension becomes a
claim cell with a strength (`strong`/`partial`/`weak`/`absent`) and a basis
(`reported`/`measured`/`derived`/`inferred`), capped by the descriptor.

- `examples/coverage-aware-drift-annotation/` — annotates a drift report.
- `examples/coverage-aware-side-effect/` — the single-archive sample shape.
- The comparator's `--coverage-annotation-out` emits the annotation sidecar
  (see `docs/experiments/cross-runtime-drift-2026-05/compare/drift.py`); the
  sidecar is separate so the drift report contract is never mutated.

### 4. Enforcement — refusing to assert what the annotation won't support
A downstream gate reads the annotation plus a set of asserted claims and returns
a deterministic pass/fail. A `positive` claim needs a measured cell at
`strong`/`partial`; an `exhaustive` claim needs the descriptor to allow it; a
`bounded_negative` claim is only evaluable on a measured dimension and must not be
blocked. This is where honesty becomes mechanical rather than documentary.

- `examples/coverage-claims-gate/` — the gate (exit `0` permit / `1` blocked).

### 5. Aggregation — the honesty posture across many runs
The same per-run classification folds across a whole set of runs into a fleet
summary: per dimension, the strength distribution and the **fleet floor** — the
weakest positive strength seen anywhere in the set. One degraded run pulls the
floor down, which is exactly the conservative behaviour you want.

- `examples/coverage-fleet-summary/` — the local fold and fleet floor.

### 6. Attestation shape (synthetic) — where verifiable evidence would slot in
A clearly-labelled synthetic demonstrator explores the *shape* a verifiable,
subject-bound claim could take, and shows the same discipline: an attested claim
degrades unless a verifiable envelope binds to the same subject. It implements no
real mechanism and changes no schema.

- `examples/attested-shape-demo/` — synthetic composition shape, demonstrator only.

## Reading it as one chain

```
capture (kernel observation)
   -> coverage descriptor (completeness ceiling)
      -> annotation (claim cells: strength x basis, capped by descriptor)
         -> enforcement (gate permits only supported claims)
         -> aggregation (fleet floor across runs)
         -> (synthetic) attestation shape (where verifiable evidence would raise the basis)
```

At every arrow the rule is the same: missing or weaker evidence lowers the claim;
nothing in the chain can quietly raise it. That is the whole point — the system
is built to be honest about what it does not know.

## Try it

Each example is stdlib-only and self-contained. Run any stage's tests with:

```bash
python3 -m unittest discover -s examples/coverage-claims-gate    -p 'test_*.py'
python3 -m unittest discover -s examples/coverage-fleet-summary  -p 'test_*.py'
python3 -m unittest discover -s examples/attested-shape-demo     -p 'test_*.py'
```
