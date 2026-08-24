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

## Local checker

`scripts/ci/assay_review_record_check.py --self-test` pins the record
contract. `--pr N` talks to the live GitHub API when `GITHUB_REPOSITORY`
and `GITHUB_TOKEN` are set. It reads the PR head, then comments, then
the PR head again; a sha/ref change is `head_moved`. Responses are
capped at 8 MiB, HTTP timeout is 30s, and comments stop after two
pages (200 comments) with `comments_limit`. A comments-API failure is
`comments_api_failure`. The pre-commit hook is
`assay-review-record-self-test`.

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
record. A later push triggers `synchronize`; the old record is stale and the
new head needs a new independent review record before a rerun can pass.

The workflow structure is cross-pinned from the existing required CI and
host-capability roots. Classic branch protection requires its stable
`review-record-check` context, and
`.github/rulesets/main-required-ci-contexts.json` records that expected live
set. The scheduled reconciliation workflow detects drift between that file and
the live protection rule.

## Non-claims

The record is not cryptographic agent identity, intellectual adequacy, review
quality, an approval count, or AGENTS carry-forward. The workflow does not
support merge queues, write comments or statuses, or use a write token. API
failure is a failed required check, not evidence that the review was defective.
A base checkout protects the executed repository code, not the PR-supplied
workflow definition. Coordinated mutation of the workflow, checker, both
required roots, and live protection is outside repo-local enforcement.
