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

`builder.agent` must match the PR branch prefix (`codex/`, `claude/`,
`cursor/`, `ruley/`). Same family is allowed. The checker rejects only
an identical `(agent, instance)` pair. `reviewer.github_login` must
equal the comment author. GitHub `Bot` comments fail. Editing the
current-head comment fails (`updated_at != created_at`). Older-head
records stay as history.

`READY` is the only passing verdict. `BLOCKED` must parse and still
fail. Empty `findings` requires `no_findings: true`; each finding is
`{id, summary, disposition}`. Independence flags must be JSON `true`,
not the string `"true"`. `reviewer` must be an object.

## Local checker

`scripts/ci/assay_review_record_check.py --self-test` pins the record
contract. `--pr N` talks to the live GitHub API when `GITHUB_REPOSITORY`
and `GITHUB_TOKEN` are set. A comments-API failure is `comments_api_failure`.
The pre-commit hook is `assay-review-record-self-test`.

## Non-claims

The record is not cryptographic agent identity, not intellectual
adequacy, not an approval count, and not AGENTS carry-forward. Slice A
does not add a required branch-protection context and does not claim a
live `review-record-check` job.
