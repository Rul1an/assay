# RFC-004: Open Items Convergence Plan (Q1 2026)

- Status: Active
- Date: 2026-02-09
- Owner: DX/Core
- Scope: Remaining open items after RFC-002 (E1-E4) delivery and RFC-003 G1-G5 execution
- Inputs:
  - `docs/architecture/CODE-ANALYSIS-REPORT.md`
  - `docs/architecture/RFC-001-dx-ux-governance.md`
  - `docs/architecture/RFC-002-code-health-remediation-q1-2026.md`
  - `docs/architecture/RFC-003-generate-decomposition-q1-2026.md`

## 1. Context

Most remediation slices are merged. The remaining risk is not broad technical debt anymore, but "last mile" convergence:

1. One still-open generate decomposition PR (G6).
2. Status drift across RFC documents.
3. A small set of high-impact structural items still open (mainly monitor monolith and typed error-boundary completion).

This RFC consolidates only those open items with explicit gates and merge order.

## 2. Verified Baseline (as of 2026-02-09)

### 2.1 Merged tracks

- RFC-002 E1-E4: merged
  - E1: `#242`
  - E2: `#245`, `#246`
  - E3: `#247`, `#250`, `#252`
  - E4: `#253`, `#254`, `#255`, `#256`
- RFC-003 Generate decomposition:
  - G1: `#260` merged
  - G2: `#262` merged
  - G3: `#264` merged
  - G4: `#266` merged
  - G5: `#268` merged
  - Finite validation hardening: `#270` merged

### 2.3 Mechanical status table (source of truth)

| Item | Status | Reference | Merge SHA | Date |
|------|--------|-----------|-----------|------|
| RFC-002 E1 | Merged | PR #242 | `d9afdc70` | 2026-02-09 |
| RFC-002 E2 | Merged | PR #245 | `39448078` | 2026-02-09 |
| RFC-002 E3A | Merged | PR #247 | `ae6e76c4` | 2026-02-09 |
| RFC-002 E3B | Merged | PR #250 | `34a03810` | 2026-02-09 |
| RFC-002 E3C | Merged | PR #252 | `e06f2458` | 2026-02-09 |
| RFC-002 E4A | Merged | PR #253 | `47c9c6b3` | 2026-02-09 |
| RFC-002 E4B | Merged | PR #254 | `574d9316` | 2026-02-09 |
| RFC-002 E4C | Merged | PR #255 | `c9f67b19` | 2026-02-09 |
| RFC-002 E4D | Merged | PR #256 | `54dff1ee` | 2026-02-09 |
| RFC-003 G1 | Merged | PR #260 | `99588b59` | 2026-02-09 |
| RFC-003 G2 | Merged | PR #262 | `545fcd09` | 2026-02-09 |
| RFC-003 G3 | Merged | PR #264 | `059e23d2` | 2026-02-09 |
| RFC-003 G4 | Merged | PR #266 | `a661b911` | 2026-02-09 |
| RFC-003 G5 | Merged | PR #268 | `b3d386bf` | 2026-02-09 |
| RFC-003 finite validate | Merged | PR #270 | `7cc96a8a` | 2026-02-10 |
| RFC-003 G6 | Open | PR #271 | - | Open |
| Docs auto-update | Open | PR #272 | - | Open |

### 2.2 Open PRs

- `#271` `refactor(generate-e5-g6): make generate.rs orchestrator-only` (open)
- `#272` `docs: auto-update diagrams and crate info` (open, docs-only)

## 3. Open Items (single source of truth)

## O1 - RFC-003 G6 merge completion

- Priority: P0
- Source: RFC-003 G6
- Scope:
  - Merge `#271` after required checks.
  - Keep extract-only semantics and G1 contract invariants.
- Contract gates:
  - `cargo test -p assay-cli --test contract_generate_g1 -- --nocapture`
  - `cargo test -p assay-cli generate::tests -- --nocapture`
  - `cargo check -p assay-cli`
  - `cargo clippy -p assay-cli -- -D warnings`
- Done when:
  - `#271` is merged on `main`.
- Evidence:
  - PR: `#271`
  - Merge SHA: required in closure note
  - CI: required checks green on Linux/macOS/Windows
- Rollback:
  - Revert merge commit of `#271` if G1 contracts regress.

## O2 - Documentation status convergence

- Priority: P0
- Source: status drift between RFC-001/002/003 and Code Analysis report
- Scope:
  - Update status lines only, no behavior/code changes.
  - Ensure all four docs agree on merged/open state.
  - Treat this as a mechanical sync from GitHub merged/open facts.
- Files:
  - `docs/architecture/RFC-003-generate-decomposition-q1-2026.md`
  - `docs/architecture/RFC-002-code-health-remediation-q1-2026.md`
  - `docs/architecture/RFC-001-dx-ux-governance.md`
  - `docs/architecture/CODE-ANALYSIS-REPORT.md`
- Stop-line:
  - No reclassification of findings without a fresh audit run.
  - Every "done/merged" claim must include PR number + merge SHA + merge date.
  - Every "open" claim must include PR link or issue link.
- Done when:
  - Statuses are internally consistent and reference merged/open evidence.
- Required deliverable:
  - Add/update a single status table ("source of truth") with:
    - item id
    - status
    - PR/issue reference
    - merge SHA (if done)
    - date
- Evidence:
  - PR for O2 itself + rendered status table in diff.
- Rollback:
  - Revert O2 PR if any claim cannot be traced to merged/open evidence.

## O3 - Monitor monolith decomposition

- Priority: P1
- Source: Code Analysis finding `#8` (`commands/monitor.rs` monolith)
- Scope:
  - Freeze-first characterization tests for monitor contracts.
  - Then extract helpers in small slices (no behavior drift).
- Stop-line:
  - No stdout/stderr contract changes.
  - No exit-code/reason-code drift.
  - No platform-gating drift (Linux/non-Linux behavior remains explicit).
- Done when:
  - `monitor.rs` is split into focused helpers/modules with characterization suite green.
- Contract guardrails:
  - Tests must assert:
    - exit code
    - reason/diagnostic core field (if present)
    - stable stderr core substrings (not full stderr snapshot)
    - OS-conditional expectations (`linux` vs `not linux`)
- Evidence:
  - PR(s) + test command outputs + CI run link(s).
- Rollback:
  - Revert latest monitor extraction PR if any platform-gated contract fails.

## O4 - Typed error boundary completion (A1 closure)

- Priority: P1
- Source: RFC-001 Wave A/B risk controls
- Current gap:
  - `RunErrorKind` exists, but `classify_message`/legacy substring classification remains active in core error assignment paths.
- Scope:
  - Typed-first assignment for run/ci hot path.
  - Legacy substring classification only as explicit fallback path.
  - Stable forensic fields (`path`, `status`, `provider`, etc.) available where applicable.
- Hot path definition (normative):
  - `assay-cli`:
    - `crates/assay-cli/src/cli/commands/run.rs`
    - `crates/assay-cli/src/cli/commands/ci.rs`
    - `crates/assay-cli/src/cli/commands/pipeline.rs`
    - `crates/assay-cli/src/cli/commands/pipeline_error.rs`
  - `assay-core` boundary mapping used by these paths:
    - `crates/assay-core/src/errors/mod.rs`
- Stop-line:
  - No reason-code contract breaks.
  - No output-schema changes (`run.json`, `summary.json`, SARIF, JUnit).
- Done when:
  - Hot path no longer depends primarily on message substring classification.
  - Substring classification is used only in explicit fallback branch.
- Measurable acceptance:
  - In hot-path mapping code, no `.contains(...)`-based reason mapping except fallback.
  - Typed variants carry stable forensic fields for triage-critical cases.
- Evidence:
  - PR(s) + grep-proof for fallback-only substring use + contract test results.
- Rollback:
  - Revert typed-boundary PR if reason-code contract tests drift.

## O5 - Run/CI parity fence hardening (B1 closure)

- Priority: P1
- Source: RFC-001 Wave B risk controls
- Scope:
  - Explicit parity contract tests for run vs ci:
    - exit code
    - reason code
    - core output invariants
    - non-blocking report-write failure behavior
- Stop-line:
  - No renderer behavior merge that changes contracts without tests.
- Done when:
  - Dedicated parity fences exist and pass on CI.
- Required parity matrix:
  - Scenario 1: success path parity
    - assert: exit code + reason code + summary invariants
  - Scenario 2: config/parse fail parity
    - assert: exit code + reason code + run/summary invariants
  - Scenario 3: runtime fail parity
    - assert: exit code + reason code + non-blocking report-write behavior
  - Scenario 4: reporting write failure
    - assert: primary outcome preserved; reporting failure remains non-blocking
- Assertion rules:
  - Prefer schema/field asserts over full string snapshots.
  - If string checks are needed, assert stable core substrings only.
- Evidence:
  - PR(s) + matrix-to-test mapping + CI run link(s).
- Rollback:
  - Revert parity-fence PR if matrix coverage is incomplete or flaky.

## O6 - Optional docs auto-update PR

- Priority: P2
- Source: open docs PR `#272`
- Scope:
  - Merge if checks pass and no conflict with O2 status-sync pass.
- Stop-line:
  - Do not let generated docs reintroduce stale RFC status text.
- Ordering rule:
  - If O2 and `#272` overlap, merge O2 first, then rebase/refresh `#272`.
- Evidence:
  - PR state + conflict-free merge proof.

## 4. Execution Order

1. O1 (`#271`) merge.
2. O2 doc status convergence PR.
3. O3 monitor freeze + extraction slices.
4. O4 typed boundary completion.
5. O5 parity fence completion.
6. O6 docs auto-update merge/rebase as needed.

## 4.1 CI/Check Discipline

- Required per item:
  - command gates listed in item scope
  - CI links in PR body
  - explicit note for any environment-only failures
- Branching:
  - no stacked PRs for O2/O4/O5 unless explicitly required
  - rebase to `main` before final merge

## 5. Definition of Done (RFC-004)

RFC-004 is done when:

1. RFC-003 is fully closed through G6 merge.
2. RFC/doc status is consistent across RFC-001/002/003 + Code Analysis report.
3. Remaining P1 structural open items from the current snapshot are reduced to:
   - 0 for generate decomposition.
   - 0 for monitor monolith.
   - 0 for A1 typed-boundary blocker.
   - 0 for B1 parity-fence blocker.

### 5.1 Machine-checkable criteria

- O2:
  - each "done" status line includes PR reference and merge SHA.
- O4:
  - hot-path files contain no substring-based reason mapping outside fallback region.
- O5:
  - parity matrix scenarios each map to at least one passing test in CI.

## 6. Out of Scope

- Demo assets and workflows:
  - `demo/`
  - `my-agent/`
  - `.github/workflows/demo.yml`
- New product features unrelated to open-item convergence.
