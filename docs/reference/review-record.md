# Review record (slice A)

Assay #2561. A merge to `main` still needs the AGENTS.md non-building
exact-head review. This page documents the **public, machine-readable
record** and the local checker that proves one exists for a named head.
It does not install a GitHub Actions workflow. The workflow lands in a
later slice, after this checker is on `main`.

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

## Non-claims

The record is not cryptographic agent identity, not intellectual
adequacy, not an approval count, and not AGENTS carry-forward. Slice A
does not add a required branch-protection context and does not claim a
live `review-record-check` job.
