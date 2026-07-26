## ADR Boundary Slice

- Canonical ledger: <!-- the programme ledger named in AGENTS.md -->
- Slice:
- Builder:
- Reviewers:
- Final head SHA: <!-- every measurement below is stated against this SHA; note worktree or toolchain where a number depends on one -->
- ADR invariants affected:

## Behavior

<!-- State the externally observable change and its failure behavior. -->

## Non-Claims

- [ ] No scalar trust score or whole-action verdict
- [ ] No generic identity, delegation, federation, HTTP/OAuth, or broad MCP scan
- [ ] No provider-outcome, compliance, certification, partnership, or safe-agent claim

## Test Evidence

### RED

<!-- Command and expected failure before the production change. -->

### GREEN

<!-- Focused test, affected suite, fmt, clippy, and public-surface checks. -->

## Review Quorum

- [ ] One non-building agent review
- [ ] CodeRabbit or Copilot review on this head SHA
- [ ] If bots were unavailable for 30 minutes, a second non-building agent review is linked
- [ ] Every actionable finding is fixed or has a technical disposition
- [ ] Any delegated proof targets this exact head SHA
