# DX Roadmap: P0-P2 Implementation Plan

**Last updated:** 2026-02-07
**Scope:** 6 features across 3 priority tiers + DX polish
**EU AI Act phased dates:** 2025-02-02, 2025-08-02, 2026-08-02
**Planning assumption:** no stop-clock; roadmap tracks current phased dates.

---

## Status Overview

| Feature | Priority | Status | PR |
|---------|----------|--------|-----|
| `init --from-trace` | P0-A | Done | #174 |
| PR Comment Bot (`--pr-comment`) | P0-B | Done | #174 |
| SARIF truncation fix (E2.3) | P0 | Done | #174 |
| Fork PR fallback (E2.4) | P0 | Already existed | — |
| `next_step()` mapping (E4.1) | P0 | Already existed | — |
| GitHub Action v2.1 (pack failure contracts) | P1 | Done | #185 |
| Golden path (`init --hello-trace`) | P1 | Done | #187 |
| `generate --diff` (feature) | P1-A | Done | #177 |
| `explain` + compliance hints (feature) | P1-B | Done | #179 |
| P1-A/P1-B docs+help parity hardening | P1 | In review | #189 |
| `doctor --fix` | P2-A | Done | #184 |
| `watch` | P2-B | Done | #184 |
| `watch` hardening (determinism/tests) | P1 | Done | #188 |
| `watch` edge hardening (coarse mtime + parse fallback) | P1 | In progress | codex/p1-watch-edge-hardening |
| P0/P1 DX integration to `main` | P0/P1 | In review | #191 |
| Docs alignment + link guard | DX polish | Done | #184 |

---

## DX Scorecard (Feb 2026 Refresh)

This roadmap now tracks developer experience on five dimensions that reflect how Assay is actually adopted in CI and day-to-day engineering loops.

| Dimension | Current State | Next Concrete Move |
|-----------|---------------|--------------------|
| Time-to-first-signal | `init --hello-trace`, `doctor --fix`, `watch` are shipped and documented | Keep onboarding regression checks as a permanent gate |
| Quality-of-feedback | Exit/reason codes, `run.json`/`summary.json`, doctor/explain flows are in place | Add copy-paste rerun hints and explicit "next best action" snippets in failure paths |
| Workflow fit | GitHub Action v2.1, SARIF, PR comments, docs link guard are in place | Continue docs/help parity checks for key CLI entry points |
| Trust & auditability | Deterministic run contracts and evidence outputs exist | Continue replay bundle hardening as cross-team reproducibility surface |
| Change resilience | `watch` path diffing is deterministic; `generate --diff` and compliance explain are implemented | Close remaining watch edge hardening (coarse mtime + parse-error fallback coverage) |

---

## Architecture Decision: Leverage Existing Modules

Most building blocks already exist. The strategy is composition, not construction:

| Feature | Existing Code | New Code |
|---------|--------------|----------|
| `init --from-trace` | `generate.rs` (694 lines), `packs.rs` | ~80 lines glue |
| PR Comment Bot | `action.yml` PR comments, `summary.json`, SARIF | ~120 lines |
| `generate --diff` | `generate.rs` serialization, `similar` crate | ~200 lines diff engine |
| `explain` + compliance | `explain.rs` (1058 lines), pack `article_ref` field | ~150 lines connector |
| `doctor --fix` | `doctor.rs` (117 lines), `fix.rs` (214 lines), core doctor (424 lines) | ~60 lines bridge |
| `watch` | `--incremental` flag, run command | ~300 lines (greenfield) |

---

## P0 — Completed

### P0-A: `init --from-trace`

**Shipped in PR #174.**

```bash
assay init --from-trace trace.jsonl              # Generate policy + config
assay init --from-trace trace.jsonl --heuristics # With entropy analysis
assay init --from-trace trace.jsonl --ci github  # Also generate CI scaffolding
```

Behavior:
1. Reads trace events via `generate::read_events()`
2. Aggregates via `generate::aggregate()`
3. Generates policy via `generate::generate_from_trace()`
4. Writes `policy.yaml` (with allow/needs_review/deny counts)
5. Writes `eval.yaml` (config pointing to policy + trace)
6. Optional: CI scaffolding, `.gitignore`
7. Prints next-step commands and compliance hint

Files changed:
- `crates/assay-cli/src/cli/args.rs` — `--from-trace`, `--heuristics` on `InitArgs`
- `crates/assay-cli/src/cli/commands/init.rs` — `run_from_trace()` function

### P0-B: PR Comment Bot

**Shipped in PR #174.**

```bash
assay ci --config eval.yaml --trace-file traces/ci.jsonl --pr-comment reports/pr-comment.md
```

Generates markdown with:
- `<!-- assay-governance-report -->` marker for upsert
- Status badge (pass/fail)
- Results table (suite, tests, exit code, reason)
- Warnings in collapsible details
- Compliance conversion hint
- Version footer

CI workflow template updated with SHA-pinned:
- `peter-evans/find-comment@3eae4d37986fb5a8592848f6a574fdf654e61f9e` (v3.1.0)
- `peter-evans/create-or-update-comment@e8674b075228eee787fea43ef493e45ece1004c9` (v5.0.0)

Files changed:
- `crates/assay-cli/src/cli/args.rs` — `--pr-comment` on `CiArgs`
- `crates/assay-cli/src/cli/commands/ci.rs` — `format_pr_comment()`, writing logic
- `crates/assay-cli/src/templates.rs` — `CI_WORKFLOW_YML` with PR comment steps

### E2.3: SARIF Truncation Fix

**Shipped in PR #174.**

Bug found and fixed: `truncate_findings()` sorted ascending then truncated from the end, keeping **lowest** severity instead of highest. Fixed by sorting descending (highest first).

Default `max_results` raised from 500 to 5000 (GitHub ingests 25k, shows top 5k).

4 unit tests added:
- `truncate_no_op_under_limit`
- `truncate_30k_to_5k_keeps_highest_severity`
- `truncate_preserves_errors_over_infos`
- `default_max_results_is_5000`

Files changed:
- `crates/assay-evidence/src/lint/engine.rs`

### E2.4 / E4.1: Already Existed

- **Fork PR fallback**: `action.yml` uses `continue-on-error: true` on PR comment steps and `!github.event.pull_request.head.repo.fork` guard on SARIF upload.
- **`next_step()` mapping**: `ReasonCode::next_step()` in `exit_codes.rs` with full deterministic mapping and unit tests.

---

## P1 — Planned

### P1-A: `generate --diff` — Policy Evolution Visibility

When regenerating policy from new traces, show what changed.

```bash
assay generate --input trace.jsonl --diff           # Compare with existing policy.yaml
assay generate --input trace.jsonl --diff --dry-run  # Preview without writing
```

Output:
```
Policy diff (policy.yaml -> generated):

  files.allow:
    + /tmp/agent-workspace/**     (count: 5, risk: low)
    - /var/log/old-service.log    (removed)
    ~ /home/user/.config/*        stability: 0.7 -> 0.9

  network.allow_destinations:
    + api.newservice.com:443      (count: 12, risk: needs_review)

  Summary: +2 added, -1 removed, ~1 changed
```

Implementation:
- Add `--diff` flag to `GenerateArgs` in `args.rs`
- Add `diff_policies(old, new) -> PolicyDiff` in `generate.rs`
- Types: `PolicyDiff`, `SectionDiff` with added/removed/changed
- `similar` crate already a dependency (used in `fix.rs`)
- Match entries by pattern string, compare stability/count/risk fields

Tests:
- `diff_empty_to_populated` — everything shows as added
- `diff_removed_entries` — detect removed patterns
- `diff_stability_change` — detect stability score changes
- `diff_no_changes` — empty diff output

### P1-B: `explain` + Compliance Hints

Every violation should teach the user something. When a tool call is blocked, show which AI Act article is relevant.

```bash
assay explain --trace trace.json --policy policy.yaml --compliance-pack eu-ai-act-baseline
```

Output:
```
Timeline:
  [0] Search(query: "user data")                    allowed
  [1] Create(path: "/etc/shadow")                   BLOCKED
      Rule: deny_list
      EU AI Act: Article 15(3) - Robustness and accuracy

Compliance Coverage:
  eu-ai-act-baseline: 3/8 rules applicable (37.5%)
  For full coverage: assay evidence lint --pack eu-ai-act-pro
```

Implementation:
- Add `--compliance-pack` to `ExplainArgs`
- Extend `RuleEvaluation` with `article_ref` and `compliance_hint` fields
- Add `ComplianceSummary` type (pack name, applicable/total rules, coverage %)
- Load compliance pack via `assay_evidence::lint::packs::loader`
- Map pack rule `article_ref` fields to violation context

Article mapping table (embedded):
```
deny_list    -> Article 15(3) (Robustness)
allow_list   -> Article 12(1) (Record-keeping)
max_calls    -> Article 14(4) (Human oversight)
before       -> Article 12(2) (Traceability)
never_after  -> Article 15(1) (Safety)
sequence     -> Article 14(3) (Oversight)
```

Tests:
- `explain_with_compliance_maps_articles`
- `explain_compliance_summary_coverage`
- `explain_no_pack_no_compliance_fields` (backward compat)

---

## P2 — Completed

### P2-A: `doctor --fix` — Self-Healing Setup

Bridge between `doctor` diagnostics and `fix` auto-repair.

```bash
assay doctor --fix                    # Diagnose and offer fixes
assay doctor --fix --yes              # Auto-apply all fixes
assay doctor --fix --dry-run          # Preview fixes
```

Output:
```
Config: eval.yaml
  [E_TRACE_MISS] Trace file not found: traces/main.jsonl
    Fix: Create empty trace file? [y/N]

  [E_CFG_PARSE] Unknown field 'response_format', did you mean 'format'?
    Fix: Replace 'response_format' with 'format' in eval.yaml? [y/N]

Applied 2 fix(es). Remaining: 0 error(s).
```

Implementation (shipped):
- Added `--fix`, `--yes`, `--dry-run` to `DoctorArgs`
- Added fast-fail guard: `--yes`/`--dry-run` require `--fix`
- `run_doctor_fix()` converts diagnostics to fix suggestions via `assay_core::agentic::build_suggestions()`
- Interactive confirmations via `dialoguer` (with `--yes` override)
- Supports dry-run patch previews (unified diff)
- Adds automatic trace-file creation fix path for `E_TRACE_MISS`
- After applying: re-runs doctor diagnostics and reports remaining errors
- Uses atomic temp-file+rename writes on Unix for parse-fix edits

Fixable diagnostics:
- `E_CFG_PARSE` with typo -> field rename
- `E_TRACE_MISS` -> create empty trace file
- `E_BASE_MISMATCH` -> regenerate baseline

Non-fixable (show hints):
- Missing API keys -> print env var names
- Performance -> suggest `--incremental`

Tests:
- `doctor_fix_yes_creates_missing_trace_file` ✅
- `doctor_fix_dry_run_does_not_write_trace_file` ✅
- `doctor_yes_without_fix_fails_fast` ✅

### P2-B: `watch` — Live Feedback Loop

File watcher that re-runs tests on config/policy/trace changes.

```bash
assay watch --config eval.yaml --trace-file traces/dev.jsonl
assay watch --config eval.yaml --trace-file traces/dev.jsonl --clear
```

Output:
```
Watching: eval.yaml, policy.yaml, traces/dev.jsonl
Press Ctrl+C to stop.

[14:32:01] Running... (triggered by policy.yaml change)
  PASS  args_valid_search      (0.01s)
  FAIL  sequence_check         (0.02s)
Result: 11/12 passed
---
[14:32:15] Waiting for changes...
```

Implementation (shipped):
- Added `WatchArgs` with `--config`, `--trace-file`, `--baseline`, `--db`, `--strict`, `--replay-strict`, `--clear`, `--debounce-ms`
- Watch loop uses dependency-free polling snapshots + debounce
- Debounce is clamped to safe bounds (`50..=60000` ms)
- `collect_watch_paths()` parses config to include policy paths referenced by tests
- Watch targets are refreshed after reruns when config-derived paths change
- Reuses `run::run` internally for each rerun

Tests:
- `collect_watch_paths_includes_policy` ✅
- `normalize_debounce_ms_clamps_low_values` ✅
- `normalize_debounce_ms_clamps_high_values` ✅
- `normalize_debounce_ms_keeps_in_range_values` ✅
- `diff_paths_is_order_independent` ✅
- `diff_paths_detects_added_removed_and_modified_paths` ✅
- `coalesce_changed_paths_sorts_and_deduplicates` ✅
- `collect_watch_paths_parse_error_keeps_core_targets` ✅
- `diff_paths_detects_same_length_change_via_content_hash` ✅
- Manual testing via `assay watch --help` and local rerun loop ✅

---

## Dependency Graph

```
P0-A (init --from-trace) [DONE]     P0-B (PR Comment Bot) [DONE]
         |                                    |
         v                                    v
P1-A (generate --diff)              P1-B (explain + compliance)
         |                                    |
         v                                    v
P2-A (doctor --fix)                  P2-B (watch)
```

P0-A and P0-B are independent. P1-A builds on generate module (same as P0-A). P1-B is independent. P2-A and P2-B are independent.

Current state: P0/P1/P2 DX slices are integrated on `codex/p0-dx-magnets-clean`; integration to `main` is tracked in PR `#196`.

Recommended order from here: merge integration PR `#196` -> keep contract/onboarding gates green -> execute only deferred items when explicitly prioritized.

---

## Next Steps (Roadmap-Aligned)

1. **Merge integration PR to `main`**
   - `#196`: merge accumulated P0/P1 DX slices from `codex/p0-dx-magnets-clean`.
2. **Confirmed completed in this integration branch**
   - `#193`: `init --hello-trace` colocation with `--config` parent.
   - `#194`: `doctor --fix --dry-run` exit behavior aligned with diagnostics contract.
   - `#195`: watch/replay `RunArgs` default-drift reduction + regression coverage.
3. **Keep permanent gates + deferred boundaries**
   - Gate A (contract): keep run/summary/SARIF/JUnit + action I/O compatibility stable.
   - Gate B (onboarding): keep clean-repo -> first actionable signal under 30 minutes.
   - Deferred by design: native `notify` backend, full-repo docs link checks, cross-platform atomic write parity beyond Unix.

## Permanent Gates

- Gate A (contract): run/summary/SARIF/JUnit + Action I/O compatibility stays stable by default.
- Gate B (onboarding): clean-repo -> first actionable signal remains <30 minutes.

## Deliberate Non-Goals (Now)

These items are intentionally **not** implemented in the current slice to keep risk and review scope controlled.

| Item | Decision | Why | Revisit when |
|------|----------|-----|--------------|
| Native fs-notify watcher backend | Defer | Polling watcher is stable and dependency-free for P2; notify adds platform-specific edge cases | After deterministic watch-loop tests are in place |
| Full-repo markdown link checker | Defer | Existing docs contain legacy links; changed-files guard prevents new drift without blocking current delivery | After legacy docs cleanup sprint |
| Non-Unix atomic write parity for doctor autofix | Defer | Unix path already safe for common CI/dev path; cross-platform parity needs dedicated IO strategy and tests | Before declaring doctor autofix GA on Windows |
| `watch --once` / CI mode | Defer | Helpful but not required for current developer watch loop | When adding watch integration tests in CI |
| Dedicated IDE governance surface | Defer | Existing CLI + CI + PR surfaces already cover the core loop; separate IDE control plane adds maintenance and policy UX complexity | After Action v2.1 and drift-aware UX are stable |

---

## Conversion Hooks

Each feature includes a compliance upsell touch point:

| Location | Hook |
|----------|------|
| `init --from-trace` output | "Tip: --pack eu-ai-act-baseline" |
| PR comment body | Coverage % + "For EU AI Act compliance scanning" |
| `explain` output | Article references + "Full coverage: eu-ai-act-pro" |
| `doctor` output | Suggests compliance pack if none configured |
| SARIF properties | `article_ref` in pack rules |

---

## Verification Checklist

For each feature:
- [x] `cargo build -p assay-cli` compiles
- [x] `cargo test -p assay-cli` passes
- [x] `cargo clippy -p assay-cli -- -D warnings` clean
- [x] Help text updated (`--help` shows new flags)
- [ ] Conversion hook present in at least 1 output path
- [x] No new dependencies added for P2 watcher implementation
