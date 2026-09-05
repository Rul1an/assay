# Review identity and relay

Tracked source for the previously local `assay-stacked-pr-ops` landing helpers.
The initial import comes from the installed skill; this directory is not automatically
installed into `~/.codex/skills` and does not change a repository's required checks.

```
bash scripts/review/safe_merge.sh PR --repo OWNER/REPO \
  --record-author GITHUB_LOGIN \
  --reviewer-identity AGENT/INSTANCE \
  --review-evidence-url https://github.com/OWNER/REPO/pull/PR#issuecomment-ID \
  --confirm-findings-disposed --merge
```

The record author is its GitHub publisher, not necessarily its reviewer. A relayed
agent is named by the existing review record's `reviewer.agent/instance`. The v0
`reviewer.github_login` field is checked as the publishing account, not promoted to
an agent identity. Direct human review can use the same login for both options only
when it is not the PR author's login and the record names `agent: human` and that
login as `instance`.

Reviewer agent and instance components use a closed, single-line identifier syntax
before they enter either JSON or human-readable output. A record cannot use control
characters or Markdown delimiters to add apparent conclusions to the readiness
report.

The evidence must be an existing `assay.review-record.v0` JSON record in a comment
on this exact PR. It must declare a completed READY review on the current head and
both non-building and non-governing-spec authorship. The helper does not follow
nested links; a READY relay pointing only to itself is not a review record. Prose-only
reviews remain valid under AGENTS.md, but are not machine-supported by this bounded
adapter. Do not rewrite somebody else's review, invent an identity, or generate a
synthetic approval to satisfy this format. Unsupported records need an explicitly
reviewed adapter, not a bypass.

Record parsing and validation are imported from
`scripts/ci/assay_review_record_check.py`; the landing helper does not maintain a
second interpretation of the schema. A malformed current-head machine carrier is
reported as BLOCKED rather than discarded. Dismissed GitHub reviews do not count,
and a current-head `CHANGES_REQUESTED` review is a blocker even without verdict
prose.

This verifies the retrieved declaration, not the actual agent's independence or
identity. The operator still disposes actionable findings. `pr_landing_readiness.py`
is only a candidate report: its old candidate extraction alone is not merge
authorization. `safe_merge.sh` additionally validates the linked evidence before
calling `gh pr merge --match-head-commit`. The old `--reviewer` and
`--confirm-independent` interface is intentionally refused, rather than silently
reinterpreted.

Evidence retrieval is limited to a constructed GitHub API comment endpoint on the
selected PR, with a 30-second timeout and an 8-MiB JSON materialization ceiling.
Subprocess output goes to temporary files; this is not a disk-quota guarantee.
No evidence contents are executed. GitHub comments remain editable; this is not an
immutable attestation or protection against subsequent evidence edits.

The broader readiness queries accept only the fixed `gh pr` and `gh api` command
families used by the reporter, validate repository and branch path components, and
apply a 30-second timeout plus an 8-MiB JSON ceiling before parsing. Human-readable
output JSON-escapes every API-provided scalar; it is a display, never a second
machine-readable verdict channel.

Run synthetic tests (fake gh, no live merges):

```
python3 scripts/review/test_review_relay.py
python3 scripts/review/test_safe_merge_protocol.py
python3 scripts/review/test_pr_landing_readiness.py
```

The `review-relay-protocol-tests` pre-commit hook runs the complete set on the PR
head through Kernel Matrix CI whenever this surface changes. The trusted-base
`review-record-check` workflow continues to execute only the base checker and is
not weakened to run pull-request code with its token.

Deployment into the local skill requires independent review and an explicit
byte-verified copy of these helpers together, plus updating the skill usage examples.
Until then the old local helper must not be used for relay merges.
