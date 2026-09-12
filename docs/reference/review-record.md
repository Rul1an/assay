# Review record

Assay #2561. A merge to `main` still needs the AGENTS.md non-building
exact-head review. This page documents the **public, machine-readable
record** and the checker that proves one exists for a named head. Slice B
added the `review-record-check` GitHub Actions job. Slice C makes that stable
context required by classic branch protection after the workflow proved both
a missing-record failure and a current-record success on the same PR head.

## What to post

Post an issue comment (not the PR body). The raw hidden marker, then
exactly one JSON fence and whitespace. Copy this block as the whole
comment body:

````markdown
<!-- assay-review-record -->
```json
{
  "schema": "assay.review-record.v0",
  "head_sha": "<40-hex live head>",
  "builder": {"agent": "ruley", "instance": "writer-1"},
  "reviewer": {"agent": "cursor", "instance": "review-1", "github_login": "Rul1an"},
  "review_completed": true,
  "verdict": "READY",
  "findings": [],
  "no_findings": true,
  "independence": {
    "did_not_build": true,
    "did_not_author_governing_spec": true
  }
}
```
````

`builder.agent` is checked against the branch prefix only when the
ref is a known agent prefix plus a nonempty suffix (`codex/foo`,
`claude/…`, `cursor/…`, `ruley/…`). Same family is allowed. A bare
`ruley` branch, or any other first path component such as
`feature/fix`, treats the declared builder as self-declared, not
inferred or verified. The checker rejects only an identical
`(agent, instance)` pair.

The carrier must be `user.type == "User"` with nonempty string
`created_at` and `updated_at`. Missing type or timestamps fail.
`Bot` (and any other type) fails. Editing fails when the two
timestamps differ. Older-head records stay as history.
`reviewer.github_login` must equal the comment author.

`READY` is the only passing verdict. `BLOCKED` must parse and still
fail. Empty `findings` requires `no_findings: true`; each finding is
`{id, summary, disposition}`. Independence flags must be JSON `true`,
not the string `"true"`. `reviewer` must be an object.

## Superseding a record on the same head

Exactly one record may be current for the live head; a second one
fails `ambiguous_current`, even when the first is malformed. A reviewer
who posted a wrong record on the live head, whether malformed or with a
verdict they need to change, posts a new record with one extra field,
`"supersedes": <comment id>`. The id is the positive integer REST id of
the earlier comment, the number in its `#issuecomment-N` link. Do not
edit the old comment: an edited record fails `edited_current` whether
or not it is superseded.

The checker retires the named record only when all of these hold. A
failure is `supersede_refused`, or the new record's own validation
reason:

- the new record is valid by itself (`BLOCKED` is valid, and the gate
  still fails on it);
- the target is a parseable record naming the same live head, not an
  older-head record, a comment without a record, or an unknown id;
- the target is older: a lower comment id, and a `created_at` that is
  not later;
- one comment author posted both, and both declare the same
  `reviewer` `agent` and `instance`;
- the new record's reviewer is not the builder the target declared.

A refused supersede retires nothing. Anything still current after
retirement is counted as before, so records from two reviewers stay
`ambiguous_current`, and so does a later record that omits
`supersedes`. Chains work (C supersedes B, which supersedes A) because
every target is older. After posting, rerun the workflow as described
under "Required workflow".

This does not widen what a login can already do. The same author can
delete its own comment and repost; superseding gets the same result
and leaves the earlier record readable. It does not repair a carrier.
A comment that does not parse, has been edited, was posted by a
non-`User`, or declares another login fails as before, because either
its reviewer identity cannot be read or the carrier is the defect.
Those still need the comment deleted or a new head.

## Local checker

`scripts/ci/assay_review_record_check.py --self-test` pins the record
contract. `--pr N` talks to the live GitHub API when `GITHUB_REPOSITORY`
and `GITHUB_TOKEN` are set. It reads the PR head, then comments, then
the PR head again; a sha/ref change is `head_moved`. When no record names
the live head it also runs `git` in the checkout to derive the carry, which
is the checker's only subprocess; the refusals are `carry_objects_unavailable`,
`carry_not_upstream_merge`, `carry_not_ancestor`, `carry_merge_conflict`,
`carry_tree_mismatch` and `carry_touched_reviewed_file`. Responses are
capped at 8 MiB, HTTP timeout is 30s, and comments stop after two
pages (200 comments) with `comments_limit`. A comments-API failure is
`comments_api_failure`. The pre-commit hook is
`assay-review-record-self-test`. The supersede rule is the checker's
`resolve_supersedes`; `scripts/review/pr_landing_readiness.py` calls
it rather than restating it.

## Required workflow

`.github/workflows/review-record-check.yml` runs for `opened`, `reopened`,
`synchronize`, and `ready_for_review` pull-request events. It checks out the
PR's base SHA, verifies that checkout, self-tests the checker from that trusted
base, and then reads the live PR head and issue comments through the GitHub API.
It never checks out or executes PR-head code and has only `contents: read` and
`pull-requests: read` permissions.

Posting a comment does not itself trigger a workflow. The normal path is to
post the record while the PR is draft and then mark the PR ready for review.
For an already-ready PR, rerun the workflow in GitHub Actions after posting the
record. A later push triggers `synchronize`. If the new head is an upstream-advance
merge, the checker re-derives the two AGENTS.md carry conditions from the
commits and the older record still counts; otherwise the old record is stale
and the new head needs a new independent review record before a rerun can pass.

The workflow structure is cross-pinned from the existing required CI and
host-capability roots. Classic branch protection requires its stable
`review-record-check` context, and
`.github/rulesets/main-required-ci-contexts.json` records that expected live
set. The scheduled reconciliation workflow detects drift between that file and
the live protection rule.

## Non-claims

The record is not cryptographic agent identity, intellectual adequacy, review
quality, or an approval count. It is not a carry either: a record carries to a
later head only when the checker re-derives both AGENTS.md conditions from the
commits, and it never reads a carry claim out of the record's text. The workflow
does not support merge queues, write comments or statuses, or use a write token. API
failure is a failed required check, not evidence that the review was defective.
A base checkout protects the executed repository code, not the PR-supplied
workflow definition. `reviewer` is declared, not verified: when several agents
post through one GitHub login, the declared pair is all that separates them,
so the supersede check cannot tell a reviewer from someone declaring that
reviewer's identity, just as a single record cannot. Coordinated mutation of the workflow, checker, both
required roots, and live protection is outside repo-local enforcement.
