# Assay Agent Contract

This file is the shared operating contract for Codex, Claude Code, Cursor, and automated PR
reviewers. Tool-specific instruction files may add mechanics, but they must not restate or weaken
this contract.

## Canonical State

- Repository truth is the checked-in code and documentation.
- The public execution ledger for the active programme is named on this line: [issue #2388](https://github.com/Rul1an/assay/issues/2388).
  Name the new ledger here when one opens, and
  say so plainly here when none is active. Keep
  the number to this one line; everywhere else the contract names the role, so the next programme
  costs one edit here rather than one in every section. Nothing enforces that, so it is an
  instruction and not a guarantee — and the way it fails is quiet: the line kept pointing at a
  finished programme, which reads as an active ledger and sends handoffs to a closed issue.
- Agent chats, local plans, memories, and unpushed branches are not authoritative project state.
- Every handoff for a programme slice records the branch, PR, exact head SHA, verification, reviews,
  non-claims, and open findings in that programme's ledger. Work outside a programme — a standalone
  fix, a documentation change — has no ledger to record to, and inventing one or appending to a
  closed one is worse than the omission.

## ADR-042/043 Scope

Read these before changing evidence ingestion, evidence verification, or MCP authorization:

- [ADR-042: Evidence-first positioning](docs/architecture/ADR-042-evidence-first-positioning.md)
- [ADR-043: Evidence-chain integrity invariants](docs/architecture/ADR-043-evidence-chain-integrity-invariants.md)

The ADR-042 stop list is normative:

- no scalar trust score;
- no whole-action verdict;
- no generic agent identity, delegation, or federation layer;
- no provider-outcome verification;
- no detector catalogue or broad MCP scan;
- no compliance or safe-agent claim;
- no certification or partnership status without a checkable basis.

ADR-043 is `Accepted`. Changes to its boundaries must preserve the implementation-evidence grid,
including its explicit non-claims, or amend the decision through a new ADR.

## Branch And Worktree Ownership

- One writer owns a branch and worktree. Other agents review the diff without editing that branch.
- At most two ADR-042/043 implementation branches may be active at once.
- Use `codex/`, `claude/`, `cursor/`, or `ruley/` branch prefixes matching the writer.
- Do not implement on `main`.
- Build in the worktree's own `target/`. Cargo creates one per worktree on first build, so
  worktrees do not share one unless told to, and `/target` is git-ignored so an in-tree build
  leaves a read-only review's tree clean. A `CARGO_TARGET_DIR` you export by hand, you remove when
  the work ends.
- Remove merged branches and their worktrees only after recording the merge in the programme ledger.

## Development Discipline

- Write a failing behavioral test before changing production behavior. Run it and confirm the
  expected failure, then implement the smallest passing change.
- Treat all evidence inputs as hostile and apply resource ceilings before materialization.
- Preserve the distinction between policy decisions, observations, and outcomes.
- Never turn absence of evidence, failed validation, skipped review, or unavailable infrastructure
  into a clean result.
- Keep public strings free of private strategy, product-roadmap language, and unearned claims.
- Stage with a pathspec that names the change. The test is whether the command could pick up a file
  you did not touch: if it could, it commits a property of the moment rather than the change under
  review. `git add -A <paths the change touches>` passes; `git add -A`, `git add -A .`, `git add .`,
  `git add -u` and `git commit -am` all fail it. The list is illustrative and the test is the rule —
  the set of ways to stage a whole tree is open, so enumerating it would only look complete.
- Pin a tool version in one place, and have both the install and the invocation read that one value.
  The defect is two literals that must agree and are free to drift — an install pinning a version that
  a config file or a second workflow states again. Echo the value in the run so a version claim can be
  checked against the log, while remembering what that check does not cover: an invocation can select
  a different toolchain through an alias while the log still names the pinned one.

## Review Quorum

The builder's self-review does not count. On the final head SHA, require one non-building agent
review that actually reviews the change and records its verdict and findings. Automated reviewers
may add evidence, but they are not part of the quorum and their absence never needs a substitute.
A non-building reviewer authored or edited neither the PR's change nor any normative specification
or implementation plan governing that change, whether or not the PR cites the artifact. Prior
read-only review of that specification or plan does not itself make the reviewer a builder.

A new push invalidates the review on the prior head, and the head a review was measured on is the
head it counts for — a merge commit that brings `main` into the branch is a new head like any other.
A review is revalidated for a new head only by a recorded equivalence check with two conditions: the
new head introduces no change, to any file, that is not already on `main` — its tree is what merging
`main` into the reviewed head produces without conflicts — and the advance from the reviewed head to
the new head touched no file the review covered, meaning the PR's changed files as of the reviewed
head. The first covers the whole tree, never a file list, so content outside the reviewed files cannot
ride the carry; the second exists because an upstream change to a reviewed file changes what lands
even though it smuggles nothing, and "the branch added nothing" is a neighbouring property of "what
was reviewed is what merges", not the same one. Put both checks in the review record; without them,
the review does not carry. Rewritten history (rebase, squash) does not carry a review even when the
tree is identical: revalidation is for upstream advances only.

A review record that says it did not review is not a review. A bot that returns `COMMENTED` with
"unable to review — quota limit", or a check that reports `pass` alongside "review rate limited",
leaves no findings and no reviewer; it is unavailable infrastructure wearing the shape of a verdict,
not a pass. Read what a record says, not that it exists. Reviews count as artifacts bound to an exact
head, not as entries in GitHub's review list; a reviewer whose tooling cannot submit a review record
counts through a comment bound to the SHA it reviewed.

Auto-merge may be enabled only when:

- all required checks are green;
- any delegated runner proof targets the final head SHA;
- the review quorum is satisfied;
- every actionable finding is fixed or has a recorded technical disposition.

For an ADR-042/043 slice, open the PR with the dedicated template:

```bash
gh pr create --template .github/PULL_REQUEST_TEMPLATE/adr-boundary.md
```

## Verification

Before pushing:

- run the focused test that proves the changed behavior;
- run the affected crate or integration suite;
- run `cargo fmt --all -- --check`;
- run clippy with `-D warnings` for the affected targets;
- inspect `git diff --check` and the public-surface strings.

GitHub Actions is the final integration proof. Never weaken, bypass, or relabel a required check to
make a branch mergeable.

### Measurement provenance

A measurement carries its provenance, or it is a claim with extra steps. When a number, a count, or a
pass/fail reaches a PR body, a review, or a programme-ledger entry, it states the exact SHA the
reported tree was committed as — not a branch name, which moves and which is how a stale checkout
passes for a current one — plus the worktree when more than one is active, and the binary or toolchain
when the number depends on one. Measuring before the push is the normal case; commit first and report
that SHA, rather than reporting a tree no one else can address.

The failure mode this catches is not arithmetic but attribution: the number is right and the tree,
ref, artifact, or build it describes is not. Name the artifact too when a generated form exists, since
a correct reading of a generated layer is still the wrong layer.

### One rule, one function

When the same rule has to be answered in two places — a load-time check and an execution-time check, a
display normaliser and a matching normaliser, a validator and the thing it validates against — make
one side call the other, or extract the single function both use. Do not write the rule twice.

The failure mode is drift, and it is silent: the second implementation starts as a faithful
approximation and diverges one edit at a time, so both sides keep passing their own tests while
disagreeing about a real input. PR #1948 spent five independent review rounds on exactly this, and
each round fixed one divergence while introducing another. What held was making the second site
mirror the first structurally and pinning them together with a test that asserts both answer the same
for the same input.

Where the two genuinely cannot share code, the fallback is that parity test — one test over a table of
inputs asserting agreement, not two separate test suites that never meet. Prefer one function; treat
the parity test as the compromise it is. `lycorp-jp/sim-use` states the same rule for its normalisers,
so that display and round-trip "can never drift apart".

## Tool Boundaries

- Codex owns programme-ledger coordination, CI triage, PR sequencing, and merge-state reconciliation.
- Claude Code may implement a dedicated slice in its own worktree and may review another slice in
  plan/read-only mode.
- Cursor may implement locally in its own worktree. Cursor Background Agents must not mutate auth
  or evidence-boundary code.
- Ruley (GrokBot) may implement a bounded slice in its own worktree and may post issue and PR
  updates. It must not write directly to `main` or use repository mutations as permission probes;
  verify access through read-only API metadata and perform writes only on its owned branch.
- Bugbot, CodeRabbit, and Copilot are optional reviewers, not authorities. Their findings require
  technical verification; their absence does not block the non-building-agent quorum above.
- Heartbeats may monitor status only. They must not generate, edit, commit, push, or merge code.
