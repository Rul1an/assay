# Errata for privileged-mcp-action/v0

**Applies to corpus digest `sha256:cb58ce91863f52e0568742b977f0642158453ec11bbcd25821f9171dccd03342`**, and to
nothing else. If the digest changes, this file is void until it is re-measured against the new one.

Recorded 2026-08-19. **No vector, expectation or digest is changed by this file.** A published
corpus whose digest is its identity does not get edited; it gets an erratum, and a later corpus
gets the fix.

## What reproducing this corpus does and does not establish

An independent implementation that reproduces all fourteen outcomes has demonstrated agreement on
**six** of the profile's rules. It has not demonstrated agreement on the profile.

This is not a defect in the vectors as written, nor a claim that the reference implementation is
wrong. It is a statement about **what fourteen vectors can transmit**. Where the profile promises a
rule that no vector isolates, an implementation can omit that rule, reproduce this digest, and be
indistinguishable here from one that honours it.

Measured with [`corpus-adequacy`](https://github.com/corpus-adequacy/corpus-adequacy) over the
normative comparison surface this corpus declares in `MANIFEST.json`
(`bundle_integrity`, `verdict`, `claims`):

```
6 of 25 DECLARED in-scope rules killed (24.0%). 4 declared out of scope, 31 rules declared.
control-killed. 19 mutant(s) survived. 2 KNOWN HOLES.
```

Reproduce with:

```
python3 ../../../corpus-adequacy/corpus_adequacy.py \
  ../adequacy/privileged-mcp-action-v0.manifest.json
```

## The six rules reproduction does establish

| rule | isolated by |
|---|---|
| exactly one decision record | `bad-103` |
| the decision value is in the closed vocabulary | `bad-107` |
| an unrecognised in-namespace payload schema fails closed (Stage 2) | `bad-104` |
| the decision `action.target_digest` is a well-formed sha256 | `bad-102` |
| `fail_closed` equals (`decision` == `"deny"`) | `bad-106` |
| a marker binds on the `target_digest` leg | `bad-105` |

## The nineteen it does not

Each is promised by [v0.md](../../docs/profiles/privileged-mcp-action/v0.md) and is not
discriminated by any of the fourteen vectors.

**Cardinality and shape**
- at most one observation record; at most one establish record
- the observation `caller_visible_response_digest` is a well-formed sha256
- the decision `tool.name` is a non-empty string; the observation `call.tool_name` is a non-empty string
- the event type equals the payload schema it declares
- an in-namespace event type over a payload declaring no in-namespace schema fails closed

**Closed vocabularies**
- the decision `reason` is in the closed producer set
- the decision `drift_state` is in the closed set
- the `establish_path` is in the closed set
- `establish_attempted` equals (`run_outcome` != `not_performed`)

**Marker binding**
- a marker binds on the `tool_name` leg
- a caller-visible denial marker is backed by a decision record
- a marker with a null or empty `call.target_digest` is unbindable and invalid

**Marker payload members**
- `caller_visible_error.code`, `.origin` and `.reason` are each present and non-null

**Producer non-claims**
- the decision carries the five producer non-claims verbatim
- the observation carries the four producer non-claims verbatim

## Two known holes, and why they are structural

`caller_visible_error.origin` must be `"assay-proxy"` and `caller_visible_error.code` must be
`-32042` — both legs of the Stage 3 marker triple, both promised, neither discriminated.

The cause is not an omission in any one vector. [`gen_vectors.py:103`](gen_vectors.py) hard-codes the
single marker shape and is the only marker construction site, so **no vector in this corpus can vary
either member**. Closing them requires a generator parameter, which is a new corpus.

The triple's third leg, `schema`, is decided earlier at Stage 2 and so is not discriminated by the
triple match either. Of the triple's three legs this corpus isolates none.

## Rules out of scope for this corpus

Four declared rules are excluded with stated reasons in
[`../adequacy/privileged-mcp-action-v0.manifest.json`](../adequacy/privileged-mcp-action-v0.manifest.json):
two mutate a v1 arm or an earlier stage and cannot move an outcome here even in principle, and one —
the establish-journey contradiction — emits only a *finding*, and findings are not on this corpus's
normative comparison surface. That last one is undiscriminable in principle rather than in practice,
and becomes an in-scope hole the moment findings join the surface.

## What this changes for an implementer

Nothing about the exercise. Reproducing the fourteen outcomes remains the task, the expectations are
unchanged, and a reproduction is still worth having: it is the only independent evidence that the
specification text is sufficient to build against.

What changes is the claim you can make afterwards, and the claim we can make about you. A successful
reproduction establishes agreement on the six rules above. Anyone stating more than that — including
us — is overstating what fourteen vectors measured.

## How this was found

Not by inspection. By mutation adequacy: delete one declared rule from the implementation, rebuild,
and see whether the corpus notices. A rule the corpus cannot notice is a rule it cannot ask a third
party to implement.

The first three declarations of that measurement were themselves wrong — the first declared three
rules and scored nothing, the second declared eight and published a pattern claim that an
adversarial re-measurement refuted. `../INDEX.md` carries that sequence, including the run whose
control survived and which therefore said nothing at all.
