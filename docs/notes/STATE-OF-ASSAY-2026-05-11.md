# State of Assay — 2026-05-11

> **Status:** operational snapshot
> **Last updated:** 2026-05-11
> **Scope:** records the post-`v3.10.0` Assay posture and the remaining
> maintenance-mode focus areas; not a roadmap or feature plan.

Snapshot of where Assay sits after the spring 2026 hardening sweep. Intended as
internal reference and as cross-repo handoff context (especially for
Assay-Harness, which is now the next governance focus).

The previous snapshot is
[`STATE-OF-ASSAY-2026-03-15.md`](../STATE-OF-ASSAY-2026-03-15.md).

## Headline

Assay has shifted from **hardening target** to **stable base layer**.

The credibility gaps that previously made Assay the bottleneck — release-truth,
workflow-security, scanner noise, Node 24 deadline-risk — are closed. The
release tag is `v3.10.0` (2026-05-11). The next priorities live above Assay,
not in it.

## What is done

These were the things blocking Assay from being treatable as a stable
compiler-layer dependency. They are now closed:

- **Workflow security** — high-confidence `zizmor` clean on `origin/main`.
  Permissions tightened, template-injection paths closed,
  `persist-credentials: false` baseline, trusted-org pinning policy
  enforced by `.github/workflows/workflow-security.yml`.
- **Release truth** — semver and publishing pipeline now self-consistent.
  Release preflight is no longer ad-hoc. Public crate policy enforced via
  `scripts/ci/check-public-crate-policy.sh`. Release assets validated by
  `scripts/ci/check-release-assets.sh`.
- **Security tab** — code-scanning alerts at zero. Deliberately-dismissed
  test-analysis alerts are tracked in
  [`SCANNER-DISMISSALS.md`](./SCANNER-DISMISSALS.md) so the dismiss-state
  remains traceable rather than implicit.
- **Node 24 readiness** — third-party Action versions on Node 24-compatible
  major lines. Local test runtime on Node 20 LTS, separate decision.
- **Trust Basis surface** — diff/report disciplined; sandbox/MCP-proxy
  hardening landed.
- **Evidence portability** — the three-family receipt line (tested /
  decided / inventoried) plus the experimental acted-family scripts are
  reproducible against released `v3.10.0`.

## What remains, but is second-order

Items that are now follow-up housekeeping rather than blockers. None of
these prevent treating Assay as a stable dependency.

- **Dependency convergence** — workspace crates and `Cargo.lock` are clean,
  but periodic upgrade sweeps still produce dozens of dependabot PRs per
  month. Continue the existing weekly cooldown-protected cadence.
- **Workflow inventory and TTL** — there are still several CI workflows
  whose purpose is implicit. An inventory note (one row per workflow:
  *what it gates, why it exists, when to revisit*) would close that.
- **Unsafe code atlas** — the repo has explicit `unsafe_code` exceptions
  outside eBPF as well as the eBPF crate itself (for example CLI/test
  seams, ring-buffer parsing, tracing/config shims, and MCP tests). A
  short atlas note listing these bounded exceptions would make the policy
  legible for future contributors without implying that the current
  boundary is eBPF-only.
- **`$GITHUB_PATH` / `persist-credentials` compatibility** — two
  compatibility decisions are documented in line comments inside specific
  workflows but not consolidated. A consolidation pass would help, but is
  not urgent.

## What the next focus actually is

With Assay stable, the next-most-likely-to-rattle surfaces are above and
beside Assay-core:

1. **Assay-Harness alignment.**
   Harness `v0.4.0` is release-clean and `zizmor`-clean but still has
   weaker governance than Assay (no required-status-check ruleset until
   2026-05-11, dependency freshness on `@openai/agents`, no Node 24 lane).
   Bringing Harness to Assay-level governance is the most concrete next
   piece of work.
2. **Packaging / proof / seeding.**
   The proof page, longform, and assurance-mapping note can now be
   surfaced with more confidence because the underlying release-truth
   and security-tab posture is clean. This is external-visibility work,
   not core engineering.
3. **New substantive slices.**
   Only after (1) and (2) are stable. Adding new functionality while
   Harness governance is still maturing risks reopening the same gaps
   on a new surface.

## Discipline rules in maintenance mode

To keep Assay in stable-base-layer shape:

- **No broad cleanup PRs.** Cleanup is now case-by-case. "We were
  already in here" is not a justification.
- **Each new follow-up PR cites a concrete trigger.** Either a real
  dependency upgrade, an audit-found gap, or a decision that needs
  documenting. No anticipatory polish.
- **Scanner dismissals are logged.** Every dismissed code-scanning alert
  has a row in `SCANNER-DISMISSALS.md`. Test-analysis state is bounded,
  not implicit.
- **Cross-repo decisions are written down here.** When Harness pulls
  in something from Assay that needed a compatibility decision, that
  decision lives in this note family (`STATE-OF-ASSAY-*.md`) rather
  than scattered in commit messages.

## Cross-references

- [Assay-Harness `v0.4.0` release](https://github.com/Rul1an/Assay-Harness/releases/tag/v0.4.0)
- [Assay `v3.10.0` release](https://github.com/Rul1an/assay/releases/tag/v3.10.0)
- [Evidence Receipts in Action](./EVIDENCE-RECEIPTS-IN-ACTION.md)
- [Evidence Receipt Assurance Mapping](./EVIDENCE-RECEIPT-ASSURANCE-MAPPING.md)
- [Claim Compilation Below AX](./CLAIM-COMPILATION-BELOW-AX.md)
