# Publication artefacts (Slice 5)

> **Status:** drafts only. Live baselines are now committed, but no
> blog post is published and no follow-up issue is filed.
>
> The vocabulary question for this experiment is *deliberately
> held* until [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162)
> gets a triage signal. The four runtime-evidence attributes filed
> there are runtime-agnostic by design, so the cross-runtime drift
> evidence either strengthens that same case or motivates a parallel
> discussion under whichever umbrella the maintainers route us to.

## Files

| File | Audience | Channel | When to ship |
|---|---|---|---|
| [`blog-draft.md`](blog-draft.md) | Engineers working on agent-runtime selection, eBPF / agent-runtime observability | Personal blog / dev.to / Hashnode | Live baselines are committed; still wait for at least one OpenInference maintainer response on #3162 before publishing. |
| [`discussion-draft.md`](discussion-draft.md) | OpenInference / OTel GenAI semconv maintainers | Comment on [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162), **only if** maintainers ask for concrete examples. Not a separate issue. | Only if triage on #3162 explicitly asks for a concrete example of cross-runtime drift surfacing through their proposed vocabulary. Otherwise the draft stays on disk. |

## Sequencing

1. Wait for OpenInference triage on #3162. Do not @-mention any
   individual maintainer. Do not promote on Slack / Discord / X
   without an explicit signal from the community.
2. Live baselines are already committed under [`../runs/`](../runs/README.md)
   and reflected in [`../findings.md`](../findings.md). Do not repeat
   the synthetic-only framing in outbound text.
3. If #3162 triage routes to OTel semconv, mention this experiment
   in the routing comment as evidence — do not open a parallel
   issue.
4. If #3162 triage stays open with no inhibitory signal, publish
   the blog after the findings doc is live-data-substituted. The
   blog cross-links #3162 once. Do not file a new issue.

## Discipline from the plan doc

These drafts honour:

- The plan doc's "External question packet" section: ask the
  smallest narrow question, one community at a time, with the
  full evidence package linked once.
- The "What we deliberately do NOT ask" list: no adoption ask, no
  integration ask, no "have you seen this in general?" question,
  no individual maintainer pings.
- The Threats to Validity from the plan doc — both drafts repeat
  the live-data / Linux-only / single-host / single-snapshot caveats
  up front so reviewers do not have to chase them down.

## What this experiment deliberately does NOT publish

- No claim that runtime A is "safer" or "better" than runtime B.
  The drift report is descriptive.
- No claim that the provider-host whitelist is exhaustive.
- No claim about latency, token cost, or model output quality —
  all explicitly out of scope from the plan doc.
- No paper, no podcast, no Twitter thread. The blog post is
  optional; the on-disk experiment package is the source of
  truth.
