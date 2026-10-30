# Code-scanning dismissal ledger

> **Status:** live ledger
> **Last updated:** 2026-10-30
> **Scope:** records intentionally dismissed code-scanning alerts and their
> revisit triggers; not a general security-debt tracker.

Tracks deliberately-dismissed scanner alerts so a future reviewer can see
that the dismiss-state is bounded test-analysis context rather than buried
real debt.

Format: one row per alert, terse. The ledger is *not* a substitute for the
GitHub Security tab — it is a human-readable trail that survives outside
the GitHub UI and answers the question:

> *"Why is this alert dismissed and when should it be re-examined?"*

## Ledger

| Date | Alert (tool · rule · ref) | Reason | Revisit trigger |
|---|---|---|---|
| 2026-05-11 | `assay-evidence-lint` · `EU12-002` · Security alert #3 (`/tmp/test_non_compliant.tar.gz`) | Stale non-compliant fixture SARIF upload from commit `bc0fe07f`; intentional test-analysis state, not a tracked repo artifact or current workflow output. | Next major Assay release, next evidence-lint audit cycle, or any reappearance on a tracked repo path. |
| 2026-05-11 | `assay-evidence-lint` · `EU12-003` · Security alert #4 (`/tmp/test_non_compliant.tar.gz`) | Stale non-compliant fixture SARIF upload from commit `bc0fe07f`; intentional test-analysis state, not a tracked repo artifact or current workflow output. | Next major Assay release, next evidence-lint audit cycle, or any reappearance on a tracked repo path. |
| 2026-05-11 | `assay-evidence-lint` · `EU12-004` · Security alert #5 (`/tmp/test_non_compliant.tar.gz`) | Stale non-compliant fixture SARIF upload from commit `bc0fe07f`; intentional test-analysis state, not a tracked repo artifact or current workflow output. | Next major Assay release, next evidence-lint audit cycle, or any reappearance on a tracked repo path. |

| 2026-10-22 | CodeQL · `py/command-line-injection` · alerts [1364](https://github.com/Rul1an/assay/security/code-scanning/1364), [1365](https://github.com/Rul1an/assay/security/code-scanning/1365) | Test-only fixture: Rust explicitly sets `ASSAY_BIN` to Cargo's binary under test; request/config/trace data cannot select it. Direct fixture execution uses the invoking operator's own authority. [Independent disposition](https://github.com/Rul1an/assay/pull/3403#issuecomment-6340676786). | Fixture becomes a production entry point, harness stops overriding `ASSAY_BIN`, or lower-trust input can select the executable. |
| 2026-10-30 | CodeQL · `py/command-line-injection` · alert [1374](https://github.com/Rul1an/assay/security/code-scanning/1374) | Capture test: Rust overrides `ASSAY_BIN` with Cargo's binary under test; the fixed wrapper receives one executable argument, not a payload-selected command. Standalone use has the invoking operator's authority. [Independent disposition](https://github.com/Rul1an/assay/pull/3437#issuecomment-6530841831). | The harness override disappears, lower-trust input selects the executable, or the fixture becomes a privileged production entry point. |
| 2026-10-30 | CodeQL · `py/path-injection` · alert [1375](https://github.com/Rul1an/assay/security/code-scanning/1375) | The wrapper's output root is operator configuration: tests use a fresh temporary directory, and the probe requires a fresh task-owned directory under an ordinary UID. Payloads cannot select it. Exclusive creation and final-component symlink refusal do not claim parent-directory race resistance or sandbox containment. [Independent disposition](https://github.com/Rul1an/assay/pull/3437#issuecomment-6530841831). | Lower-trust input selects the root, directory ownership assumptions change, or a privileged service invokes the helper. |

## Conventions

- **Date**: when the dismissal was made.
- **Alert**: enough context to find it again — tool (CodeQL / zizmor /
  etc.), rule id, and a stable reference (alert number or location).
- **Reason**: one sentence on why this is not real debt. Honest. If the
  reason is "we accept the risk for now", say so explicitly — do not
  dress it up as "false positive."
- **Revisit trigger**: a concrete condition or date when this should be
  re-examined. "Next major release" is acceptable; "later" is not.

## When to remove a row

When the underlying alert is genuinely resolved (code change, tool
upgrade, framework migration) the row stays for one further audit cycle
as historical context, then can be pruned. The git history retains
everything; this ledger is for current-state legibility.

## When NOT to use this ledger

- Real, actionable findings — fix the code; do not log a dismissal.
- Stale alerts where the underlying file or branch was deleted — those
  resolve themselves and need no row here.
- Findings outside the security-tab/code-scanning surface — for those,
  use the appropriate ADR or design note family.
