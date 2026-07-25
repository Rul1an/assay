# Assay ADR Boundary Review

For changes touching evidence ingestion, evidence verification, MCP server authorization, or
ADR-042/043, read `AGENTS.md` and the two ADRs before reviewing.

Report concrete behavioral or security defects. In particular, check:

- whether untrusted bytes are bounded before any full materialization;
- whether exact-limit input is accepted and limit-plus-one is rejected;
- whether an unverified, lint, stdin, or no-verify path bypasses the same source ceiling;
- whether missing, invalid, or unavailable evidence is interpreted as success;
- whether policy decisions, observations, and outcomes are collapsed into one verdict;
- whether stdio code consumes token-like `initialize` fields as authorization;
- whether any `ASSAY_AUTH_*` configuration can silently disable enforcement or fall back to
  permissive behavior;
- whether a token consumed by standalone mode can be logged or re-emitted on an outbound surface;
- whether transparent proxy relay bytes are incorrectly interpreted as Assay-authenticated identity;
- whether public output adds any compliance or safe-agent claim;
- whether public output adds certification or partnership status without a checkable basis;
- whether public output adds scalar trust or whole-action claims;
- whether fuzz targets exercise the evidence-bundle verifier rather than an adjacent format.

Do not recommend HTTP/OAuth, generic identity, federation, broad MCP scanning, or a new detector
catalogue in an ADR-042/043 PR. Those are explicit scope expansions, not incidental fixes.

A passing or skipped check is not proof of semantics. Tie every finding to a reachable code path,
testable behavior, or violated ADR invariant.
