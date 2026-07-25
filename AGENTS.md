# Assay Agent Contract

This file is the shared operating contract for Codex, Claude Code, Cursor, and automated PR
reviewers. Tool-specific instruction files may add mechanics, but they must not restate or weaken
this contract.

## Canonical State

- Repository truth is the checked-in code and documentation.
- The public execution ledger for the ADR-042/043 program is
  [issue #1847](https://github.com/Rul1an/assay/issues/1847).
- Agent chats, local plans, memories, and unpushed branches are not authoritative project state.
- Every handoff records the branch, PR, exact head SHA, verification, reviews, non-claims, and open
  findings in the ledger.

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

ADR-043 remains `Proposed` until its bounded-ingest, honest stdio-auth-boundary, and
evidence-verifier-fuzz slices are merged and linked from the ADR.

## Branch And Worktree Ownership

- One writer owns a branch and worktree. Other agents review the diff without editing that branch.
- At most two ADR-042/043 implementation branches may be active at once.
- Use `codex/`, `claude/`, or `cursor/` branch prefixes matching the writer.
- Do not implement on `main`.
- Do not share `target/` directories between active worktrees. Use `CARGO_TARGET_DIR`.
- Remove merged branches and their worktrees only after recording the merge in issue #1847.

## Development Discipline

- Write a failing behavioral test before changing production behavior. Run it and confirm the
  expected failure, then implement the smallest passing change.
- Treat all evidence inputs as hostile and apply resource ceilings before materialization.
- Preserve the distinction between policy decisions, observations, and outcomes.
- Never turn absence of evidence, failed validation, skipped review, or unavailable infrastructure
  into a clean result.
- Keep public strings free of private strategy, product-roadmap language, and unearned claims.

## Review Quorum

The builder's self-review does not count. On the final head SHA, require:

1. one non-building agent review plus CodeRabbit or Copilot; or
2. if both bots are skipped, rate-limited, or unavailable for 30 minutes after the last push, two
   non-building agent reviews.

`skipped` is not `pass`. A new push invalidates reviews on the prior head.

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

## Tool Boundaries

- Codex owns issue #1847 coordination, CI triage, PR sequencing, and merge-state reconciliation.
- Claude Code may implement a dedicated slice in its own worktree and may review another slice in
  plan/read-only mode.
- Cursor may implement locally in its own worktree. Cursor Background Agents must not mutate auth
  or evidence-boundary code.
- Bugbot, CodeRabbit, and Copilot are reviewers, not authorities. Their findings require technical
  verification; their absence is handled by the review fallback above.
- Heartbeats may monitor status only. They must not generate, edit, commit, push, or merge code.
