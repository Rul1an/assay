# Publication artifacts (Slice 4)

> **Status:** OpenInference question filed on 2026-05-25 as
> [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162).
> Blog draft is **not yet published** and waits on maintainer
> response per the sequencing rules below.
> Files live in the repo so the on-disk framing matches the public
> ask, and so the maintainer (Rul1an) can keep both pieces in sync
> after the conversation moves.

## Files

| File | Audience | Channel | When to ship |
|---|---|---|---|
| [`openinference-discussion.md`](openinference-discussion.md) | OpenInference / OTel GenAI WG maintainers | **Filed 2026-05-25** as [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162). Discussions were not enabled on the target repo, so it landed as an Issue under the `semantic conventions` umbrella. | n/a — already filed. File is kept on disk as the source-of-truth copy of what was posted. |
| [`blog-draft.md`](blog-draft.md) | Engineers working on OTel-family AI observability, eBPF / agent-runtime observability | Personal blog / dev.to / Hashnode. Optional cross-post link from the OpenInference issue if the conversation goes well. | After [#3162](https://github.com/Arize-ai/openinference/issues/3162) has a maintainer response. Posting the blog before the issue would skip the channel discipline the experiment plan committed to. |

## Sequencing

1. **Done.** Filed the OpenInference question as
   [`Arize-ai/openinference#3162`](https://github.com/Arize-ai/openinference/issues/3162)
   (Issues, since Discussions are not enabled on that repo). One
   question, one evidence link, no maintainer @-mentions.
2. Wait for triage. Only cross-post to
   `open-telemetry/semantic-conventions` if OpenInference
   maintainers explicitly route us there. Do not double-file
   without that routing signal.
3. After at least one maintainer response on #3162 (positive,
   negative, or "this lives elsewhere"), publish the blog with
   the issue link embedded.
4. Do not @-mention any individual maintainer. Do not promote on
   Slack / Discord / X without an explicit signal from the
   community that they want a broader audience.

## Discipline from the plan doc

These drafts honour:

- The plan doc's "External question packet" section: ask the
  smallest narrow question, one community at a time, with the
  full evidence package linked once.
- The "What we deliberately do NOT ask" list: no adoption ask, no
  integration ask, no "have you seen this in general?" question,
  no individual maintainer pings.
- The Threats to Validity from the plan doc — both drafts repeat
  the Linux-only / kernel-conditioned / measurement-boundary
  caveats up front so reviewers don't have to chase them down.
