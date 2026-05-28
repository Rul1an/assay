# Experiment Arc Lifecycle Guide

> **Status:** reference guidance for Assay experiment arcs. This
> document describes the lifecycle pattern proven by the
> Runner-vs-OTel overhead arc and the agent-observability fidelity arc.
> It does not open a new arc, define a schema, promote artifacts, or
> require all future arcs to use every step.

## Purpose

Assay experiments now have a repeatable shape: predeclare the question,
build only the harness needed to answer it, keep artifacts
experiment-scoped until a real consumer appears, and close with a
citation-oriented findings summary. This guide captures that shape so a
future arc does not have to rediscover the same review discipline.

Use this guide when an experiment may produce claims that future docs,
papers, release notes, or downstream consumers could cite. Do not use
the full lifecycle for ordinary feature work unless the feature output
will be interpreted as evidence.

## Lifecycle

| Stage | Purpose | Exit signal |
|---|---|---|
| 0. Governance | Decide naming, artifact-family status, and promotion boundaries before adding new outputs. | New artifacts have an `assay.experiment.<arc>.<artifact>.v<N>` shape or a documented reason not to. |
| 1. Plan | State the narrow question, prerequisites, acceptance rules, non-claims, and stop conditions. | Review can tell what would count as success, boundary, or inconclusive before code exists. |
| 2. Harness | Implement the smallest local or synthetic harness that exercises the planned claim classes. | Outputs validate against experiment-scoped schemas and policy rules are enforced by tests or schema conditionals. |
| 3. Carrier | Add a reviewable carrier only if raw artifacts are too scattered to inspect safely. | Evidence packs or stable directories carry artifacts without strengthening the underlying claim. |
| 4. Delegated Gate | Run real infrastructure only when a finding needs delegated evidence, not just local fixture behavior. | Health, calibration, join, and artifact retention requirements pass or the result is explicitly inconclusive. |
| 5. Findings Summary | Close with a stable citation point separate from slice history. | One `findings-summary.md` states findings, non-claims, and reproduction pointers. |
| 6. Tail Items | Keep optional follow-ups trigger-only. | Optional issues remain open but do not keep the arc itself open. |

Not every arc needs every stage. A docs-only feasibility arc may stop at
the plan. A feature-like carrier may iterate without delegated gates. A
measurement arc that publishes real findings should usually reach a
findings summary.

## Plan Rules

A plan should answer six questions before implementation starts:

1. What exact claim is being tested?
2. What artifacts or schemas can express that claim?
3. What health, calibration, or join gates must pass before the claim is
   interpretable?
4. What outcomes are success, boundary, or inconclusive?
5. What does the experiment explicitly not claim?
6. What is the smallest next harness gate?

The plan should include non-claims near the claim they bound. If a row
could be mistaken for a product ranking, security attribution, or
throughput claim, say so in the plan before the harness exists.

## Harness Rules

Harnesses should make acceptance rules mechanical wherever possible.

- Prefer schema enums, `if`/`then` rules, and tests over prose-only
  reviewer discipline.
- Treat missing or partial coverage as first-class output when absence
  is the finding.
- Keep synthetic fixtures visibly synthetic. Do not call them delegated
  evidence.
- Record source snapshots for moving upstream vocabularies.
- Keep experiment outputs under `assay.experiment.*` unless a promotion
  PR names a consumer.

The useful test is not "does the harness run?" but "can the harness
emit a row that prevents the wrong claim?"

## Delegated Gate Rules

Delegated runs should be narrow. They prove a specific publication gate
under real infrastructure; they are not a license to broaden the
finding.

Before dispatch, pin:

- workflow and input values;
- required artifacts or proof-pack references;
- host, kernel, and Runner health gates;
- calibration or retention gates;
- join-key invariants;
- expected claim class;
- non-claims for what the delegated run does not publish.

If the delegated gate is inconclusive, stop the dependent findings. A
failed or inconclusive positive baseline is not itself a semantic gap or
performance boundary.

## Dual Proof Track

Sometimes an experiment dispatch exposes an infrastructure or Runner
bug. Fixing that bug creates a second proof requirement:

- **Research evidence:** the narrow, predeclared proof needed by the
  experiment question.
- **Engineering compliance:** the broader lane or regression proof
  needed because code changed while pursuing the experiment.

Keep these tracks separate. Commit the narrow research evidence only
when it matches the experiment acceptance rules. Record broader
compliance proof in the PR conversation or engineering docs when it is
needed for merge policy. Do not widen the experiment evidence just
because the code fix required a broader validation lane.

The Slice 7 delegated semantic-gap baseline is the reference example:
the `openai-agents-kernel-policy` run verified the positive baseline,
while a separate `gates=all` run proved the cgroup fix safe for runner
lane policy.

## Findings Summary Rules

Close an arc with a citation-oriented summary when:

- the predeclared closure criteria are satisfied or explicitly
  inconclusive;
- the remaining follow-ups are optional or trigger-only;
- the useful statement can be expressed without walking through every
  slice.

A findings summary should include:

- scope and host/workload boundaries;
- numbered findings;
- non-claims attached to the findings they bound;
- a "what the findings mean together" section;
- reproduction pointers to the detailed slice history, harnesses,
  schemas, and follow-up issues.

Do not use a findings summary to promote experiment schemas, publish a
product ranking, or imply that optional tail work is required for the
arc to be closed.

## Promotion Rules

Arc closure is not schema promotion. An experiment-scoped artifact
should stay experiment-scoped after a successful findings summary unless
one of the promotion triggers in
[`namespace-governance.md`](namespace-governance.md) fires.

Common non-triggers:

- many commits landed;
- a findings summary is useful;
- a synthetic harness is complete;
- a PR reviewer liked the shape.

Common triggers:

- a CLI or production feature consumes the artifact;
- two independent arcs need the same shape;
- an external downstream consumer asks for the contract;
- a public reference surface needs a stable citation target.

## Stop Rules

Stop an arc when the original question is answered, bounded, or made
inconclusive by a predeclared gate. Do not keep widening just because
another adjacent question is interesting.

Good stop outcomes:

- **Healthy through threshold:** no boundary found within the declared
  budget.
- **First boundary:** boundary found with nearest healthy predecessor.
- **Inconclusive at current budget:** required health, calibration, or
  join gate failed.
- **Carrier/prototype complete:** feature-like carrier has enough shape
  for the next consumer to test.

Follow-up questions should become trigger-only issues or a new arc with
their own plan, not hidden extensions of the closed arc.

## Non-Claims

- This guide does not require every experiment to become a long slice
  series.
- This guide does not promote any `assay.experiment.*` schema.
- This guide does not make evidence packs a product API.
- This guide does not replace issue-specific acceptance rules.
