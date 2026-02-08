# DX Implementation Plan — Default Gate Readiness

**Status:** Living plan (updated after Wave A merge)
**Date:** 2026-02-08
**Source:** Critical DX review of [DX-REVIEW-MATERIALS.md](DX-REVIEW-MATERIALS.md); aligns with [ADR-019 PR Gate 2026 SOTA](architecture/ADR-019-PR-Gate-2026-SOTA.md) and [ROADMAP](ROADMAP.md). Aangepast na SOTA/DX reality check: technische correcties (GitHub Actions ref, SARIF limits, exit-codes compat), P0 Go/No-Go checklist, scope trims (E6a/E6b, cost guardrails, scrubbing deny-by-default). Score na aanpassingen: 9.7/10.

This document turns the DX review into a concrete backlog with **per-file patchlist** and test cases. Work is ordered P0 (must-have before default gate) then P1 (SOTA).

---

## RFC-001 Execution Track

Canonical RFC for debt-ranked execution:
- [RFC-001: DX/UX & Governance](architecture/RFC-001-dx-ux-governance.md)

PR order for the new track:
1. PR-A1: typed error boundary + centralized reason-code mapping (Wave A start).
2. PR-A2: remove strict-mode env mutation (`set_var`) in run/ci path.
3. PR-A3: canonical config writing hardening (`init`/templates + docs).
4. PR-B1/B2/B3: pipeline unification + coupling reduction + `--pack` to `--preset`.
5. PR-C*: perf/scale only when benchmark data justifies it.

Current blocker gates (re-assessed on implemented code):
- **Wave A blocker**: A1 must become truly typed at classification boundary (stable fields first, substring fallback explicit/legacy only).
- **Wave A blocker**: A1 boundary errors need stable forensic fields (path/status/provider) to avoid message-only support triage.
- **Wave B blocker**: B1 requires explicit run-vs-ci parity contract tests for exit/reason and output invariants.
- **P2 alerts (non-blocking)**: replay coupling wording update, A2 scope clarity (run/ci vs CLI-wide), B3 deprecation timeline as governance.

Current branch focus:
- PR-A1 (merged to `main` via #198): typed boundary mapping for run/ci hot-path triage with unit coverage.
- PR-A2/A3 (merged to `main` via #202): strict-mode env mutation removal + canonical init/template config writing.
- PR-B1/B2/B3 (merged to `main` via #204/#205/#209): pipeline unification + dispatch decoupling + `--preset` rename with compat aliases.
- Wave C kickoff:
  - PR-C0 (#212, open): additive performance trigger metrics + Wave C trigger guardrails in RFC-001.
  - PR-C1 (#213, in review): reproducible verify/lint perf harness + workload budgets (`docs/PERFORMANCE-BUDGETS.md`).
  - PR-C2 (current branch): reduce runner per-task clone overhead and emit `runner_clone_ms` from core run artifacts.

---

## P0/P1 Epic Execution Summary

Compact execution view for all P0/P1 workstreams.

| Epic | Priority | Status | Outcome |
|------|----------|--------|---------|
| EP0-1 Blessed Init + CI Template Contract | P0 | Done | `assay init --ci` paved road + workflow contract |
| EP0-2 CI Feedback Contracts (JUnit/SARIF/report I/O) | P0 | Done | stable CI outputs, robust reporting behavior |
| EP0-3 Exit/Reason Contract | P0 | Done | deterministic exit/reason surfaces for automation |
| EP1-1 GitHub Action v2.1 (compliance-pack first) | P1 | Planned (Next) | Action v2.1 P1 slice on existing PR/CI surfaces |
| EP1-2 Golden Path (<30m first signal) | P1 | Planned | init bootstrap: hello-trace + smoke suite |
| EP1-3 Explain + Compliance Hints | P1 | In review | feature delivered; parity/contract hardening pending |
| EP1-4 Drift Visibility (`generate --diff`) | P1 | In review | feature delivered; parity/contract hardening pending |
| EP1-5 Watch Determinism Hardening | P1 | Planned (Hardening-only) | existing watch behavior hardened for determinism/edge cases |
| EP1-6 Privacy-safe Observability Defaults | P1 | Planned | redaction/cardinality defaults and tests |
| EP1-7 MCP Auth Hardening (E6a hard scope) | P1 | Planned | OAuth/JWT/JWKS no-pass-through baseline |
| EP1-8 Replay Bundle Hardening | P1 | Planned | reproducible evidence bundle + manifest discipline |

Recommended sequence:
1. EP1-1 GitHub Action v2.1 (compliance-pack support).
2. EP1-2 Golden Path (<30m first signal).
3. EP1-3 + EP1-4 parity hardening (docs/examples/contract tests; no feature expansion).
4. EP1-5 Watch hardening (determinism + Windows/file edge cases + loop tests).
5. EP1-6/EP1-7/EP1-8 parallel where capacity allows.

Explicit deferred boundaries:
- no native notify watcher backend now;
- no full-repo docs link checker as hard CI gate;
- no non-Unix atomic-write parity expansion in this slice;
- no dedicated IDE governance control-plane in this phase.

## No-Regression Gates (Permanent)

Gate A (contract stability):
- `run.json` / `summary.json` contracts, SARIF/JUnit outputs, and GitHub Action I/O remain backward-compatible by default.

Gate B (onboarding velocity):
- clean repo -> first actionable Assay signal remains under 30 minutes on documented golden path.

Any P1 epic that violates A or B must either:
- include an explicit migration plan, or
- be split so contract/onboarding stability lands first.

## P1 DX Contract Surfaces

Per epic we define what is normative (stable contract) versus best-effort (implementation detail).

### EP1-1 Action v2.1 (compliance-pack first)
- Normative:
  - compliance-pack resolution behavior and logged resolved pack reference.
  - distinct failure modes: missing pack vs invalid pack vs lint/policy fail.
  - output parity across Action surfaces (summary/SARIF/JUnit).
- Best effort:
  - internal caching strategy for pack resolution.
  - non-contractual log phrasing.

### EP1-2 Golden Path (<30m first signal)
- Normative:
  - documented bootstrap flow must produce an actionable first signal.
  - generated scaffold commands in docs must execute as written.
  - regression gate enforces onboarding time budget.
- Best effort:
  - exact sample fixture contents.
  - cosmetic scaffold formatting.

### EP1-3 Explain + Compliance Hints (in review)
- Normative:
  - `--compliance-pack` behavior and compatibility with non-pack mode.
  - article hint + coverage summary field presence in supported output modes.
  - failure output includes concrete next-action guidance.
- Best effort:
  - wording of explanatory prose.
  - ordering of non-contractual detail lines.

### EP1-4 Drift Visibility (`generate --diff`) (in review)
- Normative:
  - stable added/removed/changed semantics for drift output.
  - deterministic diff output for identical inputs.
  - `--diff` does not alter existing write semantics without explicit write flags.
- Best effort:
  - pretty-print formatting and grouping style.
  - optional metadata lines.

### EP1-5 Watch hardening (existing command, hardening only)
- Normative:
  - debounce clamp range and trigger-coalescing behavior.
  - watch-loop exit semantics (loop lifecycle vs run result logging).
  - config parse failure fallback: keep watching at least config/trace/baseline.
- Best effort:
  - polling interval tuning.
  - filesystem timestamp granularity handling nuances.

### EP1-6 Privacy-safe observability defaults
- Normative:
  - safe-by-default redaction and cardinality guardrails are on by default.
  - unsafe raw prompt/body exposure requires explicit opt-in configuration.
  - default exports do not leak prompt/response bodies.
- Best effort:
  - exact redaction text tokenization strategy.
  - non-contractual telemetry attribute ordering.

### EP1-7 MCP auth hardening (E6a)
- Normative:
  - RFC 8707 resource/audience constraints enforced.
  - JWT alg/typ/crit validation and JWKS rotation behavior enforced.
  - no-pass-through token behavior enforced.
- Interop matrix (required):
  - JWKS rotation / kid miss.
  - alg confusion + typ/crit rejection.
  - audience/resource mismatch handling.
- Best effort:
  - cache refresh cadence internals.
  - diagnostics verbosity.

### EP1-8 Replay bundle hardening
- Normative:
  - verify/scrub defaults are safe and on by default.
  - bundle manifest captures deterministic replay-critical metadata.
  - unsafe/raw capture paths require explicit opt-in.
- Best effort:
  - archive layout details that do not affect verification/replay contract.
  - optional manifest annotation fields.

---

## Progress Update (2026-02-08)

Recent implementation state:

- **Wave A merged to `main`:**
  - `#198` (A1): centralized run/ci error classification via typed boundary helpers.
  - `#202` (A2/A3 integration): strict-mode env mutation removal + canonical scaffold/config writing.
- **P0/P1 DX slices merged earlier to `main`:**
  - docs/CLI parity, `doctor --fix`, `watch` hardening, Action v2.1 pack contracts, and follow-up parity checks.
- **Deferred by design (unchanged):**
  - native `notify` backend,
  - full-repo docs link checks as hard gate,
  - cross-platform atomic-write parity beyond Unix.

Roadmap-aligned next execution order from here (after Wave B1 has landed):
1. **Wave B2:** reduce `commands/mod.rs` coupling and replay dependency on re-exports.
2. **Wave B3:** rename `init --pack` to `--preset` (migration-safe CLI/docs update).
3. Re-evaluate Wave C only with measured bottlenecks.

Explicit "do not implement now" decisions:
- Do not migrate to a native notify watcher yet (keep dependency-free polling in place).
- Do not switch to full-repo docs link validation yet (keep changed-files guard).
- Do not broaden doctor atomic-write guarantees beyond Unix in this slice.
- Do not add a dedicated IDE governance control plane yet (focus on CLI/CI/PR surfaces first).

### Post-#191 Follow-up Plan

After integration PR `#191` lands in `main`, execution continues in three narrow follow-up slices to avoid scope creep:

1. **PR A: init hello-trace colocation**
   - Branch: `codex/p1-init-hello-trace-colocation`
   - Change: make `assay init --hello-trace` write `traces/hello.jsonl` relative to the directory of `--config`.
   - Acceptance:
     - `assay init --hello-trace --config /tmp/x/eval.yaml` creates `/tmp/x/traces/hello.jsonl`.
     - Existing default flow remains unchanged for local `eval.yaml`.

2. **PR B: doctor dry-run exit contract**
   - Branch: `codex/p1-doctor-dry-run-exit-contract`
   - Change: align `doctor --fix --dry-run` exit codes with documented diagnostics contract.
   - Acceptance:
     - Dry-run still writes nothing.
     - Exit code semantics are explicit and consistent across code, tests, and docs.
     - `doctor_fix_e2e` expectations match the final contract.

3. **PR C: watch RunArgs drift reduction (optional)**
   - Branch: `codex/p1-watch-runargs-builder`
   - Change: reduce/manual `RunArgs` duplication in watch execution path to avoid default drift over time.
   - Acceptance:
     - No behavior change in watch output/exit semantics.
     - Refactor is covered by existing watch/run tests.

Delivery guardrails for all three follow-ups:
- Keep slices independent and reviewable.
- Do not change run/summary/action output contracts unless explicitly intended and documented.
- Update `docs/DX-ROADMAP.md` status immediately after each merge.

EU AI Act date anchors used in this plan:
- 2025-02-02: first phased obligations active.
- 2025-08-02: GPAI-focused obligations active.
- 2026-08-02: broader obligations active.

## DX North Star (2026)

Use this scorecard as a gate for roadmap choices. If a new item does not clearly improve at least one dimension below, it is de-prioritized.

| Dimension | Practical Target | Current Baseline | Planned Work |
|-----------|------------------|------------------|--------------|
| Time-to-first-signal | First actionable result in <30 min | Good docs and commands, but no guaranteed hello-trace bootstrap | Golden-path hardening in init/templates |
| Quality-of-feedback | Every failure routes to a next action | Reason codes + doctor/explain exist | Add explicit rerun/next-action hints in outputs and PR surfaces |
| Workflow fit | Native PR/CI/Security integration | Action v2 + SARIF + PR comments already in place | Action v2.1 compliance-pack support first |
| Trust & auditability | Reproducible and shareable evidence | Deterministic outputs and reason-code contracts exist | Replay bundle hardening and stronger manifest usage |
| Change resilience | Drift visible before breakage | Watch refresh and docs alignment are in place | `generate --diff` + drift-aware explain output |

### Execution Filters

- Prefer paved-road improvements over adding new interfaces.
- Keep policy gate decisions deterministic; keep reporting failures non-blocking where possible.
- Prioritize low-cognitive-load defaults (self-service templates over manual config work).
- Treat SARIF, run/summary JSON, and Action inputs/outputs as compatibility contracts.

---

## Default Gate Go/No-Go Checklist (P0)

> **Zodra alle items hieronder groen zijn: "default gate ready".**

| # | Criterium | Test/Verificatie | Status |
|---|-----------|------------------|--------|
| 1 | **init template uses v2 action** | `assay init --ci` → `.github/workflows/assay.yml` bevat exact `Rul1an/assay/assay-action@v2` (golden/contract test) | ✅ |
| 2 | **SARIF always has locations** | Unit test: elk SARIF result heeft `locations.length ≥ 1` | ✅ |
| 3 | **SARIF schema contract test** | SARIF output passes schema 2.1.0 validation | ✅ |
| 4 | **Exit codes aligned** | Missing trace → exit 2 + `E_TRACE_NOT_FOUND`; judge unavail → exit 3 + `E_JUDGE_UNAVAILABLE` | ✅ |
| 5 | **reason_code everywhere** | reason_code in: console, job summary, summary.json; `reason_code_version: 1` in summary.json | ✅ |
| 6 | **summary.json stable** | `schema_version` + `reason_code_version` in output; golden test | ✅ |
| 7 | **JUnit path contractual** | `.assay/reports/junit.xml` (of gekozen pad) in docs + tests + action | ✅ |
| 8 | **Compat switch documented** | `--exit-codes=v2` (default) / `v1` (legacy) + `ASSAY_EXIT_CODES` env in run.md | ✅ |

**Definition of "default gate ready":** All ⬜ → ✅

---

## 0. Epics Overview

De onderstaande epics groeperen het DX-plan in uitvoerbare eenheden. Per epic: **goal**, **priority** (P0/P1), **stories**, **acceptance criteria**, **effort**. De gedetailleerde patchlist staat in de secties 1–8.

---

### Epic E1: Blessed init & CI on-ramp

| | |
|---|--|
| **Goal** | Eerste 15 minuten: één duidelijke, blessed flow van init tot CI; geen template drift. |
| **Priority** | P0 (1.1, 1.2), P1 (1.3) |
| **Effort** | P0: ~1 dag; P1: +1–2 dagen |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E1.1 | Template v2: `assay init --ci` genereert `.github/workflows/assay.yml` met `Rul1an/assay/assay-action@v2` (moving major tag) of exact tag/SHA; geen v1-referentie | P0 | §1.1 |
| E1.2 | Blessed entrypoint: documenteer `assay init --ci` als blessed, `assay init-ci` als alias | P0 | §1.2 |
| E1.3 | One-click DX demo repos: `examples/dx-demo-node`, `examples/dx-demo-python` (minimal app, workflow, baseline, README) | P1 | §1.3 |
| E1.4 | Golden-path bootstrap: `assay init` genereert optioneel hello-trace fixture + smoke suite voor snelle first signal | P1 | §1.2/§1.3 |

**Acceptance criteria:**

- [ ] `assay init --ci` → `.github/workflows/assay.yml` bevat `assay-action@v2` (golden/contract test).
- [ ] Docs: init --ci = blessed; init-ci = alias; CI-integration + example repos link.
- [ ] (P1) CI of smoke: `assay run` in dx-demo-node en dx-demo-python slaagt.
- [ ] (P1) `assay init` kan een minimale trace + suite scaffolden die lokaal direct een bruikbaar signaal geeft.

---

### Epic E2: PR feedback UX (JUnit, SARIF, fork)

| | |
|---|--|
| **Goal** | PR-native feedback: JUnit-annotaties, SARIF upload die niet faalt, duidelijke grenzen bij fork PRs. |
| **Priority** | P0 (2.1 locatie + contract, 2.2), P1 (2.2 limits, 2.3 fork) |
| **Effort** | P0: ~1–2 dagen; P1: +0,5 dag |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E2.1 | JUnit default + blessed snippet: use `assay ci --junit ...`; run.md snippet "failures as annotations" + "where is junit.xml" | P0 | §2.1 |
| E2.2 | SARIF location invariant: elk result ≥1 location (synthetic fallback); contract test (schema + upload-smoke) | P0 | §2.2 |
| E2.3 | SARIF limits: truncate + "N results omitted" bij overschrijding GitHub-limits; configureerbaar | P1 | §2.2 |
| E2.4 | Fork PR: documenteer "geen SARIF/comment, wel job summary"; action al conditioneel | P1 | §2.3 |

**Acceptance criteria:**

- [ ] JUnit artifact + annotations bij failure met blessed snippet.
- [ ] Unit: elk SARIF-result heeft `locations.length ≥ 1`; contract: schema 2.1.0 + upload-smoke.
- [ ] (P1) Truncatie + N omitted in run summary/SARIF description.
- [ ] (P1) Docs: fork = job summary only.

---

### Epic E3: Exit codes & reason code registry

| | |
|---|--|
| **Goal** | Geen DX-landmine: exit 3 = infra/judge; trace not found = exit 2 + E_TRACE_NOT_FOUND; machine-readable reason codes overal. |
| **Priority** | P0 |
| **Effort** | ~1 dag |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E3.1 | Error/reason code registry: E_TRACE_NOT_FOUND, E_JUDGE_UNAVAILABLE, E_CFG_PARSE, etc.; mapping naar exit 0/1/2/3 | P0 | §3 |
| E3.2 | summary.json: `schema_version`, **`reason_code_version: 1`**, `reason_code` (+ message); versioned en stabiel | P0 | §3 |
| E3.3 | **Compat switch:** `--exit-codes=v2` (default na migratie), `--exit-codes=v1` (legacy, optioneel deprecation warning); env `ASSAY_EXIT_CODES=v1|v2` voor CI | P0 | §3 |
| E3.4 | reason_code in **alle** outputs: console (laatste regels), job summary, summary.json, SARIF ruleId/helpUri (indien van toepassing); downstream tooling op reason_code schakelen, niet op exit code | P0 | §3 |
| E3.5 | Docs + deprecation: run.md, troubleshooting.md, ADR-019 compatibility | P0 | §3 |

**Acceptance criteria:**

- [ ] Missing trace → exit 2, reason_code E_TRACE_NOT_FOUND (v2); v1 legacy beschikbaar via --exit-codes=v1.
- [ ] Judge unavailable (mock) → exit 3, reason_code E_JUDGE_UNAVAILABLE.
- [ ] summary.json bevat reason_code_version; reason_code in console, job summary, summary.json (en waar van toepassing SARIF).
- [ ] run.md en troubleshooting.md in lijn met gedrag; ADR-019 compatibility beschreven.

---

### Epic E4: Ergonomie & debuggability

| | |
|---|--|
| **Goal** | Elke fout met concrete next step; performance-DX (slowest 5, cache, phase timings); progress N/M. |
| **Priority** | P1 |
| **Effort** | ~1–2 dagen |

**Stories:**

| ID | Story | Priority | Detail ref | Status |
|----|-------|----------|------------|--------|
| E4.1 | Next step in errors: `suggest_next_steps(exit_code, reason_code, context)` in run/ci/doctor; troubleshooting per-error next steps | P1 | §4.1 | |
| E4.2 | Performance DX: slowest 5 tests, cache hit rate, phase timings in console + summary.json | P1 | §4.2 | |
| E4.3 | Progress UX: N/M tests, optioneel ETA in console | P1 | §4.3 | ✅ PR #164 |

**Acceptance criteria:**

- [ ] Config/trace/test failure → stdout bevat minstens één suggestie (assay doctor / explain / baseline).
- [ ] summary.json bevat slowest_tests (max 5), cache_hit_rate, phase_timings; console toont ze.
- [x] Suite met 10+ tests → console toont progress (bijv. 3/10). ✅ PR #164 (JoinSet, throttle, formatter tests).

---

### Epic E5: Observability & privacy defaults

| | |
|---|--|
| **Goal** | Default geen prompt/response-export; in 2026 "table stakes". Concreet: prompts/response bodies nooit in OTel events, replay bundles, SARIF, job summary; alleen hashes/digests of truncated safe snippets opt-in. |
| **Priority** | P1 |
| **Effort** | ~0,5 dag (naast P1 SOTA OTel) |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E5.1 | Privacy default: do-not-store-prompts default on; **concreet** nooit in: OTel events, replay bundles, SARIF, job summary; alleen hashes/digests of truncated safe snippets opt-in | P1 | §5 |
| E5.2 | **Golden tests** op exports: default config → geen prompt/response body in OTel, replay, SARIF, summary | P1 | §5 |

**Acceptance criteria:**

- [ ] Golden tests: export (OTel, replay, SARIF, job summary) met default bevat geen prompt/response body.

---

### Epic E6: P1.3 MCP Auth Hardening (Security baseline)

| | |
|---|--|
| **Goal** | OAuth 2.0 Security BCP; RFC 8707 resource; geen pass-through; JWT alg/typ/crit; JWKS + DPoP hardening. |
| **Priority** | P1 SOTA (**E6a = hard P1**, **E6b = optional P1+**) |
| **Effort** | E6a: 2 dagen; E6b: +1 dag (optioneel, feature flag) |

**Scope split (beheersbare delivery):**

| Tier | Scope | Rationale |
|------|-------|-----------|
| **E6a (hard P1)** | Resource indicators (RFC 8707), iss/aud/exp/nbf, JWKS caching + rotation + kid-miss + max-keys, alg whitelist (RS256/ES256), typ check, crit reject, no pass-through | Core security baseline; hard invariant |
| **E6b (optional P1+)** | DPoP + jti replay cache; htu/htm strict checks | Sender-constrained tokens; edge cases; feature flag `auth.require_dpop: bool` |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E6a.1 | Resource indicators (RFC 8707): resource/iss/aud/exp/nbf; JWKS cache + rotation | **P1 (hard)** | §8.1.1, 8.1.5 |
| E6a.2 | Alg/typ/crit hardening: whitelist RS256/ES256; typ check; unknown crit → reject | **P1 (hard)** | §8.1.3 |
| E6a.3 | No pass-through: incoming token nooit doorgegeven; downstream altijd eigen token + ander aud | **P1 (hard)** | §8.1.6 |
| E6b.1 | DPoP (optioneel): jti replay cache; htu/htm strict; behind feature flag | **P1+ (optional)** | §8.1.2, 8.1.4 |
| E6.4 | Negative test suite: token validation, alg/typ/crit, JWKS rotation, resource mismatch, no pass-through, DPoP replay | P1 | §8.1.6 |

**Acceptance criteria:**

- [ ] **E6a DoD:** resource + iss/aud; alg/typ/crit tests; JWKS stale-while-revalidate + kid-miss + max-keys; no pass-through bewezen; config gedocumenteerd.
- [ ] **E6b DoD (optional):** DPoP jti replay cache + htu/htm strict (when enabled via feature flag).

---

### Epic E7: P1.1 Judge Reliability MVP

| | |
|---|--|
| **Goal** | Minder flaky CI: borderline band, randomized order default, rerun on instability, 2-of-3, policy per suite type. |
| **Priority** | P1 SOTA |
| **Effort** | 2–3 dagen (+1 tuning) |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E7.1 | Borderline band + rerun strategy: TwoOfThree, triggers = borderline + low_margin + order_flip + high_variance | P1 | §8.2.1, 8.2.4, 8.2.5 |
| E7.2 | Randomized order default: seed in **summary.json én job summary** (zodat reviewers direct zien); OrderStrategy config | P1 | §8.2.2 |
| E7.3 | Order-invariance + metrics: order_invariance_rate, flip_rate, abstain_rate, margin | P1 | §8.2.3, 8.2.6 |
| E7.4 | Policy per suite type: security=fail_closed, quality=quarantine, regression=fail_on_confident | P1 | §8.2.7 |
| E7.5 | Reason codes E_JUDGE_UNCERTAIN, E_JUDGE_UNAVAILABLE; exit_codes.rs + policy.rs | P1 | §8.2.8 |
| E7.6 | **Cost guardrails:** rerun is duur; **cap: `judge.max_extra_calls_per_run`** (default 2); logs warning bij limiet | P1 | §8.2 |

**Acceptance criteria:**

- [ ] DoD §8.2.10: randomized order + seed (summary.json + job summary); rerun-on-instability; **max extra judge calls per run**; config-first policies; metrics in CI-run; multi-judge placeholder.

---

### Epic E8: P1.2 OTel GenAI (Observability)

| | |
|---|--|
| **Goal** | OTel GenAI semconv compliance; version gating; low-cardinality metrics; composable redaction. |
| **Priority** | P1 SOTA |
| **Effort** | 1–2 dagen |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E8.1 | Semconv version gating: config + manifest; versioned span attributes | P1 | §8.3.1 |
| E8.2 | Spans + metrics (GenAI semconv); **low-cardinality enforcement** + **cardinality budget tests** + **"reject dynamic labels" guard** in code | P1 | §8.3.2, 8.3.3 |
| E8.3 | Composable redaction policies; golden tests default vs full | P1 | §8.3.4 |

**Acceptance criteria:**

- [ ] DoD §8.3.5: semconv version in config/manifest; cardinality tests; redaction golden tests; config observability.md.

---

### Epic E9: Replay Bundle (DX + forensic)

| | |
|---|--|
| **Goal** | Reproduceerbare run uit één artifact; toolchain + seeds in manifest; scrubbed cassettes. |
| **Priority** | P1 SOTA |
| **Effort** | 2–3 dagen |

**Stories:**

| ID | Story | Priority | Detail ref |
|----|-------|----------|------------|
| E9.1 | Bundle format + manifest: file digests, git_sha, workflow_run_id | P1 | §8.4.1 |
| E9.2 | Toolchain capture: rustc, cargo, Cargo.lock, cargo metadata, runner metadata | P1 | §8.4.2 |
| E9.3 | Deterministic seed logging: judge_order_seed, random_seed in manifest | P1 | §8.4.3 |
| E9.4 | Scrubbed cassettes policy + tests; include_prompts false default; **scrubbing "deny-by-default"** (allowlist, niet blocklist) | P1 | §8.4.4, 8.4.5 |
| E9.5 | CLI: `assay bundle create`, `assay replay --bundle [--live] [--seed N]` | P1 | §8.4.6 |

**Acceptance criteria:**

- [ ] DoD §8.4.7: toolchain + seeds in manifest; replay roundtrip; scrubbed policy getest; signature placeholder.

---

### Epics: volgorde & afhankelijkheden

| Fase | Epics | Opmerking |
|------|-------|-----------|
| **P0 (default gate)** | E1 (E1.1, E1.2), E2 (E2.1, E2.2), E3 | Parallel waar mogelijk |
| **P1 DX** | E1.3, E2.3, E2.4, E4, E5 | E4.1, E4.2, E5 kunnen parallel |
| **P1 SOTA** | E6 → E7 → E8 → E9 | E6 eerst (security); E9 gebruikt output E7/E8 |

**Totale effort (indicatief):** P0 ~3–4 dagen, P1 DX ~2–3 dagen, P1 SOTA ~8–12 dagen (zie §8.6).

---

## 1. First 15 minutes: init as blessed on-ramp

### 1.1 Template drift (v1 → v2 action in init --ci)

**Problem:** `assay init --ci` (and `assay init-ci --provider github`) generate a workflow that uses `assay-action@v1` and `assay_version: "v1.4.0"`, while the recommended and documented action is `assay-action@v2`. Trust break in minute 5.

**Fix:** Init-generated GitHub workflow MUST use the blessed v2 template. **Belangrijk:** GitHub Actions ondersteunt geen semver ranges in `uses: owner/repo@ref`. Opties: moving major tag `@v2` (aanbevolen DX-default), exact tag `@v2.12.3`, of pinned SHA voor supply-chain strictness.

| File | Change |
|------|--------|
| `crates/assay-cli/src/templates.rs` | Replace `CI_WORKFLOW_YML`: `uses: Rul1an/assay-action@v1` → **`uses: Rul1an/assay/assay-action@v2`** (canonieke vorm: action in subdirectory). Geen `version: "2.x"` (niet ondersteund); template gebruikt @v2. Optioneel comment: "Voor supply-chain strictness: pin op exacte tag of SHA + Dependabot." |
| `docs/getting-started/ci-integration.md` (or equivalent) | "assay init --ci genereert workflow met `Rul1an/assay/assay-action@v2`. Voor supply-chain strictness: pin op exacte tag of SHA; zie CHANGELOG." |
| `docs/reference/cli/init.md` | Init --ci / init-ci github schrijft de **blessed** workflow; output pad is **`.github/workflows/assay.yml`** (contractueel). |

**Test cases:**

- `assay init --ci` in empty dir → `.github/workflows/assay.yml` bevat **exact** `Rul1an/assay/assay-action@v2` en geen v1-referentie (expliciete assertion op deze string in contract test).
- `assay init-ci --provider github` → zelfde output.
- Golden snapshot van `CI_WORKFLOW_YML` in tests (e.g. `tests/fixtures/contract/`) met assertion op action path.

---

### 1.2 One blessed entrypoint: init --ci vs init-ci

**Problem:** Two ways to do the same thing (`assay init --ci` vs `assay init-ci`) weakens "one blessed flow" (ADR-019).

**Fix:** Choose one as blessed; document the other as alias.

| File | Change |
|------|--------|
| `docs/DX-REVIEW-MATERIALS.md` | In A.1, state: "Blessed: `assay init --ci` (and `assay init --ci github`). `assay init-ci --provider github` is an alias that writes the same workflow." |
| `docs/guides/user-guide.md` | Recommend `assay init --ci` for first-time setup; mention `assay init-ci` as alternative that does the same. |
| `docs/reference/cli/init.md` | Document `--ci` and `--ci github`; add "See also: assay init-ci (alias for CI-only workflow generation)." |
| `crates/assay-cli/src/cli/commands/init_ci.rs` | No code change required; optionally add a single println hint: "Tip: You can also run 'assay init --ci' for full init + CI." so both paths are discoverable. |

**Decision (to document):** Blessed = `assay init --ci`. `assay init-ci` remains as alias (no removal) to avoid breaking existing scripts.

**Test cases:**

- Both commands produce byte-identical `.github/workflows/assay.yml` when using same provider (after 1.1 is done).

---

### 1.3 One-click DX demo repos (P1)

**Problem:** No minimal Node/Python example repo that demonstrates 0 → CI gate (clone, run, PR with annotations).

**Fix:** Add two example directories with minimal app + 1 test + working workflow + baseline flow.

| File / Dir | Change |
|------------|--------|
| `examples/dx-demo-node/` | **New.** Minimal Node app (e.g. one script + one test), `assay.yaml`, `policy.yaml`, `ci-eval.yaml` (or equivalent), `.github/workflows/assay.yml` (blessed v2), `traces/` with one trace, README: "0 → CI: clone, npm install, assay run..., open PR." Include baseline: first run baseline export, CI compare. |
| `examples/dx-demo-python/` | **New.** Same idea for Python (pyproject.toml or requirements.txt, one test, assay config, workflow, traces, README, baseline flow). |
| `docs/DX-REVIEW-MATERIALS.md` | In A.2, replace "geen aparte minimale Node- of Python-voorbeeldrepo" with pointer: "See examples/dx-demo-node and examples/dx-demo-python for one-click 0→CI demos." |
| `docs/getting-started/ci-integration.md` | Add subsection "Example repos" linking to `examples/dx-demo-node` and `examples/dx-demo-python`. |

**Test cases:**

- CI job in this repo (or local) runs `assay run` in `examples/dx-demo-node` and `examples/dx-demo-python` and exits 0 (or document as manual smoke).

---

## 2. PR feedback UX

### 2.1 JUnit: default + native annotations (blessed snippet)

**Problem:** JUnit is not default in the action; no single blessed snippet for "failures as annotations" and "where is junit.xml".

**Fix:** Action heeft escape hatch (teams willen soms alleen SARIF of alleen job summary). Default "works", geen lock-in.

| File | Change |
|------|--------|
| `assay-action/action.yml` | **Action inputs:** `junit: true` (default true), `sarif: true` (default true, same-repo only), `comment: auto|always|never` (default auto). Stap die assay draait: schrijft JUnit naar **contractueel pad** `.assay/reports/junit.xml` (of configureerbaar pad). Upload artifact + één **blessed** JUnit reporter (gekozen en gepind: SHA of vaste tag) voor annotations. Pad vastgelegd in docs + tests + action. |
| `docs/reference/cli/run.md` | **"Failures as annotations"**: één blessed YAML snippet (assay run met `--junit`, upload artifact + JUnit report action). **"Where is junit.xml"**: contractueel pad `.assay/reports/junit.xml` (of `--junit` override); vastgelegd in docs + contract test. |
| `docs/DX-REVIEW-MATERIALS.md` | B.1: "Action inputs junit/sarif/comment; blessed snippet; pad contractueel." |

**Test cases:**

- Contract test: output path voor JUnit is het gekozen pad (default `.assay/reports/junit.xml`).
- CI workflow met blessed snippet produceert JUnit artifact en annotations bij failure (manual of e2e).

---

### 2.2 SARIF: always one location + upload contract + limits (P0/P1)

**Problem:** GitHub upload can fail with "expected at least one location". No contract test. No handling for result/size limits.

**Fix:**

| File | Change |
|------|--------|
| `crates/assay-core/src/report/sarif.rs` | **write_sarif:** Each result MUST include at least one `locations` entry. If no file/line from TestResultRow, use a synthetic location (e.g. `assay.yaml` or config path from context). Same for **build_sarif_diagnostics:** when `locations` is empty, use synthetic location (e.g. `"assay.yaml"` or `"policy.yaml"`). |
| `assay-evidence` (if it emits SARIF) | Same rule: every result has ≥1 location; synthetic if needed. |
| Contract test (new or in existing) | Add test: SARIF output from assay run (or build_sarif_diagnostics) is valid and accepted by GitHub upload (snapshot + schema validation; optional: real upload in CI with small result set). |
| `crates/assay-core/src/report/sarif.rs` (or report pipeline) | **Limits:** When result count or SARIF size exceeds GitHub limits, truncate and add a "N results omitted" (or similar) message in run summary / SARIF run description; configurable or default truncation threshold. |

**Test cases:**

- Unit: every result in generated SARIF has `locations` length ≥ 1.
- Contract: generated SARIF passes schema 2.1.0 and contains at least one location per result.
- Optional: CI step that uploads a minimal SARIF (1 result, 1 location) to verify upload-sarif accepts it.

---

### 2.3 Fork PR: no SARIF/comment; fallback to job summary (P1)

**Problem:** Fork PRs cannot upload SARIF or post comments (permissions). Users should get feedback only via job summary.

**Fix:** Job summary **altijd** kernresultaten bevatten, zodat devs bij beperkte permissies toch feedback zien (ook bij "expected checks" zonder artifacts).

| File | Change |
|------|--------|
| `assay-action/action.yml` | Al conditioneel op same-repo voor SARIF/comment. Expliciet in comments/docs: fork PRs = geen SARIF upload, geen PR comment. **Job summary (GitHub step summary) altijd schrijven met kernresultaten** (pass/fail count, reason_code indien van toepassing) zodat fork PR's feedback krijgen. |
| `docs/DX-REVIEW-MATERIALS.md` or CI docs | "Fork PRs: SARIF upload en PR comment worden overgeslagen (GitHub permissions). Job summary bevat altijd kernresultaten." |
| `docs/getting-started/ci-integration.md` | "On fork PRs, only the job summary is updated with core results; SARIF and PR comment require same-repo." |

**Test cases:**

- Documented behaviour; optional: trigger from fork en assert no upload/comment, summary bevat kernresultaten.

---

## 3. Exit codes: remove DX landmine (P0)

**Problem:** run.md says exit 3 = "Trace file not found"; ADR-019 wants 3 = "infra/judge unavailable". Redefining 3 breaks existing users/CI.

**Fix (SOTA):** Stable, machine-readable **reason code registry** (decoupled from exit code). Coarse exit codes 0/1/2/3; **expliciete compat switch**; reason_code in **alle** outputs; downstream tooling schakelt op reason_code, niet op exit code.

| File | Change |
|------|--------|
| `crates/assay-cli` (e.g. `exit_codes.rs`) | Reason code registry: E_TRACE_NOT_FOUND, E_JUDGE_UNAVAILABLE, E_CFG_PARSE, etc. Mapping naar exit 0/1/2/3. **Compat:** `--exit-codes=v2` (default na migratie), `--exit-codes=v1` (legacy; optioneel deprecation warning). Env **`ASSAY_EXIT_CODES=v1|v2`** voor CI. |
| Summary.json / report pipeline | Elke non-zero exit: **`schema_version`**, **`reason_code_version: 1`**, **`reason_code`** (+ message). Versioned en stabiel voor toekomstige uitbreidingen. |
| Console / job summary / SARIF | **reason_code in alle outputs:** console (laatste regels), job summary, summary.json, SARIF ruleId/helpUri waar van toepassing. Grepable debugging. |
| `docs/architecture/ADR-019-PR-Gate-2026-SOTA.md` | Compatibility: "Exit code 3 = infra/judge unavailable. Trace-not-found = exit 2 + E_TRACE_NOT_FOUND. Gebruik --exit-codes=v1 voor legacy; downstream op reason_code schakelen." |
| `docs/reference/cli/run.md` | Exit codes table 0/1/2/3; "Reason codes" → registry; "Legacy: exit 3 was 'trace file not found'; use summary.json reason_code for stable behaviour." |
| `docs/guides/troubleshooting.md` | Trace file not found onder Exit 2; Judge/infra onder Exit 3. |

**Test cases:**

- Missing trace → exit 2, reason_code E_TRACE_NOT_FOUND (v2); met --exit-codes=v1 → legacy exit 3.
- Judge unavailable (mock) → exit 3, reason_code E_JUDGE_UNAVAILABLE.
- reason_code aanwezig in console output, summary.json (incl. reason_code_version), en waar van toepassing job summary/SARIF.
- run.md and troubleshooting.md match behaviour.

---

## 4. Ergonomie & debuggability

### 4.1 Default "next step" in every error (P1)

**Problem:** Not every exit≠0 ends with 1–2 concrete commands. Te veel next steps = noise; niemand leest het.

**Fix:** **Context-aware** next steps; **max 2** per exit.

| File | Change |
|------|--------|
| `crates/assay-cli` (run/ci/doctor paths) | Centraliseer in `suggest_next_steps(exit_code, reason_code, context)`. **Context-aware** voorbeelden: E_TRACE_NOT_FOUND → "check path, run assay doctor, list traces"; E_CFG_PARSE → "assay doctor --config …"; E_JUDGE_UNAVAILABLE → "retry, check rate limits, enable VCR replay, set backoff". **Beperk tot max 2 next steps** per exit. |
| `docs/guides/troubleshooting.md` | "Next steps" per error type; elk sectie eindigt met concrete command(s); max 2 per type. |

**Test cases:**

- Trigger config error, missing trace, failing test; stdout bevat max 2 suggesties (assay doctor / explain / baseline, context-afhankelijk).

---

### 4.2 Performance-DX: slowest 5, cache hit rate, phase timings (P1)

**Problem:** No "slowest 5 tests", "cache hit rate", or "total time per phase" in console or summary.

**Fix:**

| File | Change |
|------|--------|
| `crates/assay-core/src/report/console.rs` (and summary pipeline) | Na run: slowest_tests (max 5), cache (hit_rate, hits, misses), timings (phase: ms). Stabiel schema in summary.json. |
| `docs/reference/cli/run.md` or report docs | Document summary fields: slowest_tests[], cache.{hit_rate,hits,misses}, timings.{phase}. Cap slowest 5. |

**Test cases:**

- Run suite with multiple tests; summary.json contains slowest_tests (max 5), cache, timings; console shows them.

---

### 4.3 Progress UX: N/M tests, ETA-ish (P1)

**Problem:** Long suites have no "N/M done, ETA" feedback.

**Fix:**

| File | Change |
|------|--------|
| `crates/assay-core` (runner or report) | Emit progress updates: e.g. "Running test 3/10..." and optional "ETA ~Xs" (simple linear estimate). No fancy progress bar required. |
| `docs/DX-REVIEW-MATERIALS.md` | C.4: "Progress: N/M tests, optional ETA in console." |

**Test cases:**

- Run suite with 10+ tests; console shows progress lines (e.g. 3/10).

---

## 5. Observability: privacy-safe defaults (P1)

**Problem:** GenAI events (prompt/response capture) are not everywhere; default should not export prompt/response content. In 2026 is dit "table stakes".

**Fix:** **Concreet** waar prompts/response bodies nooit mogen staan (default):

| File | Change |
|------|--------|
| Default (geen opt-in) | Prompts/response bodies **nooit** in: OTel events, replay bundles, SARIF, job summary. Alleen hashes/digests of truncated safe snippets als **opt-in**. |
| CLI / config | "do-not-store-prompts" (of equivalent) default on. Document in run/reference. |
| Tests | **Golden tests** op exports: default config → geen prompt/response body in OTel export, replay bundle, SARIF output, job summary. |

**Test cases:**

- Golden tests: OTel export, replay bundle, SARIF, job summary met default config bevatten geen prompt/response body (of alleen hash/digest indien gedocumenteerd).

---

## 6. Backlog summary (copy-paste for issues)

Elk item is gekoppeld aan een epic (zie §0).

### P0 (must-have before default gate)

| # | Epic | Item |
|---|------|------|
| 1 | E1.1 | **Template v2:** `templates.rs` CI_WORKFLOW_YML → assay-action@v2, semver pin; docs init/ci-integration align. |
| 2 | E1.2 | **Blessed entrypoint:** Document init --ci as blessed, init-ci as alias (docs only). |
| 3 | E2.2 | **SARIF locations:** assay-core (and assay-evidence if applicable) guarantee ≥1 location per result; synthetic if needed. |
| 4 | E2.2 | **SARIF contract test:** Snapshot + schema + optional upload smoke for SARIF output. |
| 5 | E3 | **Exit code 3 + registry:** Reason code registry; summary.json met schema_version + reason_code_version: 1 + reason_code; **compat switch** --exit-codes=v2 (default) / v1 (legacy), ASSAY_EXIT_CODES env; reason_code in console, job summary, summary.json, SARIF; run.md + troubleshooting.md. |
| 6 | E2.1 | **JUnit:** Action inputs junit/sarif/comment met defaults + escape hatch; run.md blessed snippet; contractueel pad .assay/reports/junit.xml; één blessed reporter gepind. |

### P1 (SOTA)

| # | Epic | Item |
|---|------|------|
| 7 | E1.3 | **DX demo repos:** examples/dx-demo-node, examples/dx-demo-python (minimal app, 1 test, workflow, baseline flow, README). |
| 8 | E2.4 | **Fork PR fallback:** Docs: fork = job summary only; action already conditional; document clearly. |
| 9 | E2.3 | **SARIF limits:** Configureerbare truncation (max results, max bytes); default safe; "N omitted"; geen magische getallen zonder config/const + docs. |
| 10 | E4.1 | **Next step in errors:** suggest_next_steps() in run/ci/doctor; troubleshooting.md per-error next steps. |
| 11 | E4.2 | **Performance DX:** slowest 5, cache hit rate, phase timings in console + summary.json. |
| 12 | E4.3 | **Progress:** N/M tests, optional ETA in console. |
| 13 | E5 | **Privacy:** do-not-store-prompts default, redaction tests. |

---

## 7. File-level checklist (patchlist)

| File / area | P0 | P1 |
|-------------|----|----|
| `crates/assay-cli/src/templates.rs` | v2 template (`Rul1an/assay/assay-action@v2` of exact tag/SHA); output `.github/workflows/assay.yml` | — |
| `crates/assay-cli/src/cli/commands/init_ci.rs` | — | Optional hint "assay init --ci" |
| `crates/assay-cli/src/cli/commands/mod.rs` or new | Error code registry, exit 3 mapping | suggest_next_steps() |
| `crates/assay-core/src/report/sarif.rs` | ≥1 location per result; synthetic fallback | Truncate + "N omitted" |
| `assay-evidence` SARIF (if any) | ≥1 location per result | — |
| `assay-action/action.yml` | — | JUnit default + annotations; fork/docs |
| `docs/reference/cli/run.md` | Exit codes + reason codes; JUnit snippet + path | — |
| `docs/guides/troubleshooting.md` | Exit 2/3 alignment | Next step per error |
| `docs/getting-started/ci-integration.md` | init v2, example repos pointer | Fork behaviour |
| `docs/architecture/ADR-019-PR-Gate-2026-SOTA.md` | Compatibility: exit 3 deprecation | — |
| `docs/DX-REVIEW-MATERIALS.md` | — | Bless init --ci; JUnit/SARIF/fork notes |
| `crates/assay-core` report/runner | — | slowest 5, cache rate, phase timings, progress N/M |
| New: contract test SARIF | Schema + location invariant | — |
| New: examples/dx-demo-node, dx-demo-python | — | Full demo repos |
| OTel / redaction | — | Default no prompt/response; redaction test |

---

## 8. P1 SOTA Implementation (Judge, Security, Observability, Replay)

**Status:** Planned (Updated: Bleeding Edge Jan 2026)
**Priority Order:** P1.3 → P1.1 → P1.2 → Replay Bundle
**Rationale:** Security baseline first (hard invariant), then judge reliability (CI signal), then observability (debugging), then DX (replay).
**Review Score:** 9.2/10 → **9.7/10** with bleeding edge additions below.

---

### 8.1 P1.3 MCP Auth Hardening (Security Baseline)

**Goal:** OAuth 2.0 Security BCP compliance + sender-constrained tokens where applicable.

#### 8.1.1 Resource Indicators (RFC 8707)

| File | Change |
|------|--------|
| `crates/assay-mcp-server/src/auth/` | Enforce `resource` parameter matches protected API; validate `iss`, `aud`, `exp`, `nbf` with configurable clock-skew window |
| `crates/assay-mcp-server/src/auth/jwks.rs` | JWKS caching with rotation support; old key revoked → reject; new key → accept |
| Config | Add `auth.clock_skew_seconds` (default 30), `auth.jwks_cache_ttl_seconds` (default 300) |

#### 8.1.2 DPoP (Sender-Constrained Tokens) — Optional Hardening

| File | Change |
|------|--------|
| `crates/assay-mcp-server/src/auth/dpop.rs` | **New.** DPoP proof validation per RFC 9449; `cnf.jkt` thumbprint binding |
| Config | `auth.require_dpop: bool` (default false for MVP, true for high-security deployments) |

#### 8.1.3 Bleeding Edge: Alg/Typ/Crit Hardening (JWT Footguns)

| Check | Implementation |
|-------|----------------|
| **Alg whitelist** | Only `RS256`/`ES256`; **reject `none`** and unexpected algorithms |
| **Typ verification** | Verify `typ` header (`JWT` or `at+jwt` depending on issuer); strict header parsing |
| **Crit handling** | If `crit` present and extension unknown → **reject** (classic bypass vector) |

#### 8.1.4 Bleeding Edge: Replay Defense (DPoP)

| Aspect | Implementation |
|--------|----------------|
| **jti replay cache** | Per `(jti, iat)` window; config `auth.dpop_jti_cache_ttl_seconds` |
| **htu/htm strict** | Validate HTTP method + URL exact match |

#### 8.1.5 Bleeding Edge: JWKS Caching "Done Right"

| Feature | Implementation |
|---------|----------------|
| **Stale-while-revalidate** | Soft TTL to avoid request spikes |
| **Kid miss → force refresh** | Unknown `kid` triggers immediate refresh (rotation path) |
| **Max key set size** | Limit on number of keys (DoS prevention); config `auth.jwks_max_keys` |

#### 8.1.6 Negative Test Suite

| Test Category | Cases |
|---------------|-------|
| **Token validation** | expired, wrong issuer, wrong audience, invalid signature |
| **alg/typ/crit confusion** | `alg=none`, unexpected algorithms, wrong `typ`, unknown `crit` extensions |
| **JWKS rotation** | old key revoked (reject), new key added (accept), cache invalidation, kid miss refresh |
| **Resource mismatch** | token `resource` ≠ requested API |
| **No pass-through (hard proof)** | incoming token never in logs/telemetry; downstream call always with different token + different `aud` |
| **DPoP replay** | jti reuse rejected; htu/htm mismatch rejected |

#### 8.1.7 Definition of Done

- [ ] `resource` enforced + `iss`/`aud` validated conform OAuth BCP
- [ ] **Alg/typ/crit confusion tests** (bleeding edge)
- [ ] JWKS with stale-while-revalidate + kid-miss refresh + max-keys
- [ ] DPoP jti replay cache + htu/htm strict (when enabled)
- [ ] "No pass-through" proven in tests (logs + downstream aud)
- [ ] Config documented in `docs/reference/config/mcp-server.md`

**Effort:** 2–3 days

**DX Impact:** Fewer "mysterious 401/403" errors — developers understand what to fix via reason codes.

---

### 8.2 P1.1 Judge Reliability MVP (CI Signal/Noise)

**Goal:** Reduce flakiness, add bias mitigation, structured uncertainty handling.

#### 8.2.1 Borderline Band + Adaptive Calibration

| File | Change |
|------|--------|
| `crates/assay-core/src/judge/borderline.rs` | **New.** `BorderlineBand { lower: f64, upper: f64 }` with default 0.4–0.6; per-suite/model calibration from historical variance |
| `crates/assay-core/src/judge/mod.rs` | Integrate borderline detection before final verdict |
| Config | `judge.borderline_band: [0.4, 0.6]` (overridable per suite) |

#### 8.2.2 Bleeding Edge: Randomized Order as DEFAULT

Instead of always A/B → B/A test: **randomized order (with seed) is DEFAULT** in CI for pairwise comparisons.

| File | Change |
|------|--------|
| `crates/assay-core/src/judge/order.rs` | **New.** `OrderStrategy::Randomized` (default) or `Fixed` for backward compat |
| Config | `judge.order_strategy: "randomized"` (default) |
| Output | **Seed logged in summary.json én job summary** (zodat reviewers direct zien) for replay |

This makes position bias visible without extra calls.

#### 8.2.3 Order-Invariance (Bias Mitigation)

| File | Change |
|------|--------|
| `crates/assay-core/src/judge/reliability.rs` | **New.** `OrderInvariantEval`: run both A/B and B/A for pairwise judgments; aggregate with majority/score-averaging |
| Output metrics | `order_invariance_rate`, `flip_rate` (label changed over A/B vs B/A) |

#### 8.2.4 Bleeding Edge: Rerun on Instability (Not Just Borderline)

Rerun triggers expanded beyond borderline:

| Condition | Trigger | Config |
|-----------|---------|--------|
| **Borderline** | score in [0.4, 0.6] | `judge.borderline_band` |
| **Low margin** | `|score − 0.5| < ε` | `judge.margin_threshold: 0.1` |
| **Order flip** | A/B ≠ B/A verdict | automatic |
| **High variance** | std_dev > threshold | `judge.variance_threshold` |
| **Judge unavailable** | timeout/5xx | fallback policy |

```yaml
# Config example
judge:
  rerun_triggers:
    - borderline      # score in [0.4, 0.6]
    - low_margin      # |score - 0.5| < margin_threshold
    - order_flip      # A/B vs B/A disagreement
    - high_variance   # std_dev > variance_threshold
```

#### 8.2.5 Rerun Strategy (2-of-3 Majority)

```
if first_run NOT in rerun_triggers:
    return verdict (done, 1 call)
elif first_run triggers rerun:
    run second
    if first == second:
        return verdict (done, 2 calls)
    else:
        run third
        return majority(first, second, third) (done, 3 calls)
```

| File | Change |
|------|--------|
| `crates/assay-core/src/judge/rerun.rs` | **New.** `RerunStrategy::TwoOfThree` with instability triggers |
| Config | `judge.rerun_strategy: "two_of_three"` (default) or `"always_three"` |

#### 8.2.6 Output Metrics

| Metric | Description |
|--------|-------------|
| `consensus_rate` | % runs where all iterations agreed |
| `flip_rate` | % runs where label changed over iterations |
| `abstain_rate` | % runs returning "uncertain" |
| `margin` | Average distance to decision boundary |
| `order_seed` | Seed used for randomized order (for replay) |
| `effective_sample_size` | For weighted voting (future) |

#### 8.2.7 Bleeding Edge: Config-First Policies per Suite Type

| Suite Type | Uncertain Policy | Rationale |
|------------|------------------|-----------|
| **security** | `fail_closed` | uncertain = fail (security posture) |
| **quality** | `quarantine` | warn, optional human review |
| **regression** | `fail_on_confident` | fail only on confident regression, quarantine uncertain |

```yaml
# Config example
suites:
  - name: security_checks
    type: security
    uncertain_policy: fail_closed
  - name: quality_metrics
    type: quality
    uncertain_policy: quarantine
```

#### 8.2.8 Fail Modes: Split "Uncertain" from "Unavailable"

| Condition | Exit Code | Reason Code | Default Policy |
|-----------|-----------|-------------|----------------|
| Judge returns "uncertain" (instability detected) | 1 | `E_JUDGE_UNCERTAIN` | Configurable per suite type |
| Judge unavailable (timeout/5xx/rate limit) | 3 | `E_JUDGE_UNAVAILABLE` | Fail-closed with clear reason |

| File | Change |
|------|--------|
| `crates/assay-cli/src/exit_codes.rs` | Add `E_JUDGE_UNCERTAIN` reason code |
| `crates/assay-core/src/judge/policy.rs` | `JudgeFailPolicy::FailClosed`, `JudgeFailPolicy::Quarantine` per suite type |

#### 8.2.9 Future: Multi-Judge Support (Placeholder)

```yaml
# Structure for later: 2 different judge models (cheap + strong)
judge:
  models:
    - name: fast
      model: gpt-4o-mini
      role: first_pass
    - name: strong
      model: gpt-4o
      role: tiebreaker  # only on disagreement
```

#### 8.2.10 Definition of Done

- [x] **Randomized order default** with seed in summary.json + job summary
- [x] **Cost guardrails:** `judge.max_extra_calls_per_run` (default 2); warning logged when cap reached
- [x] **Rerun-on-instability** (borderline + low_margin + order_flip + high_variance)
- [x] Config-first policies per suite type (security/quality/regression)
- [x] CI-run produces `consensus_rate`, `flip_rate`, `abstain_rate`, `margin`
- [x] Reason codes `E_JUDGE_UNCERTAIN`, `E_JUDGE_UNAVAILABLE`
- [x] Multi-judge config placeholder (structure, not full implementation)
- [x] **Audit E:** Robust JSON Parsing (Greedy stream seeker)
- [x] **Audit F:** Audit Evidence Pack (E7-AUDIT.md)

**Effort:** 2–3 days (MVP), +1 day for tuning PRs

**DX Impact:** Fewer flaky failures → devs trust CI again. "Uncertain" with reason_code + next_step → faster debugging.

---

### 8.3 P1.2 OTel GenAI (Observability)

**Goal:** OpenTelemetry GenAI semantic conventions compliance; privacy-safe defaults.

#### 8.3.1 Bleeding Edge: Semconv Version Gating

**Critical:** GenAI semconv evolves rapidly. Without version gating, backward compat breaks.

```yaml
# Config
otel:
  genai_semconv_version: "1.28.0"  # or "latest"
```

| File | Change |
|------|--------|
| `crates/assay-core/src/otel/genai.rs` | Version-gated span attributes |
| `summary.json` / bundle manifest | Include which semconv mapping was used |
| Feature flag | `--features otel-genai-semconv-1.28` |

#### 8.3.2 Span Layers

| Span Type | Attributes (GenAI semconv) |
|-----------|---------------------------|
| **Provider span** (HTTP) | `http.method`, `http.url`, `http.status_code`, `http.request.duration` |
| **GenAI logical span** | `gen_ai.system`, `gen_ai.request.model`, `gen_ai.usage.input_tokens`, `gen_ai.usage.output_tokens`, `gen_ai.response.finish_reasons`, `assay.cache_hit` |

| File | Change |
|------|--------|
| `crates/assay-core/src/providers/trace.rs` | Extend with GenAI semconv attributes |
| `crates/assay-core/src/otel/genai.rs` | **New.** GenAI span builder conforming to OTel semantic conventions (versioned) |

#### 8.3.3 Bleeding Edge: Low-Cardinality Enforcement (Hard)

| Allowed Labels | Forbidden Labels |
|----------------|------------------|
| `provider`, `model`, `operation`, `outcome` | prompt hash, user id, request id, trace id |
| `verdict`, `suite_type` | file paths, dynamic strings |

| Metric | Labels |
|--------|--------|
| `assay.llm.request.duration` | `provider`, `model`, `operation` (chat/embeddings/judge), `outcome` (ok/error/uncertain/cache_hit) |
| `assay.llm.tokens.total` | `provider`, `model`, `direction` (input/output) |
| `assay.judge.decisions` | `verdict` (pass/fail/uncertain), `suite_type` (security/quality) |

| File | Change |
|------|--------|
| `crates/assay-core/src/otel/metrics.rs` | **New.** Metrics registry with above definitions |
| Tests | **New.** `test_metric_labels_bounded()` (cardinality budget); **"reject dynamic labels" guard** in code (geen prompt hash, user id, trace id, file paths als labels) |

#### 8.3.4 Bleeding Edge: Composable Redaction Policies

```yaml
otel:
  capture_prompts: false  # default
  redaction_policies:
    - strip_secrets      # API keys, tokens
    - strip_file_paths   # Local paths
    - strip_pii          # Email, phone (regex)
    - custom: "s/password=.*/password=REDACTED/"
```

| File | Change |
|------|--------|
| Config | `otel.capture_prompts: false` (default), `otel.redaction_policies: [...]` |
| `crates/assay-core/src/otel/redaction.rs` | **New.** Composable redaction policies |
| Tests | Golden tests: default = no prompt in export; `capture_prompts: true` = redacted content |

#### 8.3.5 Definition of Done

- [ ] **Semconv version gating** in config + manifest
- [ ] **Low-cardinality enforcement tests** (labels bounded)
- [ ] Spans conform GenAI semconv (versioned)
- [ ] Composable redaction policies
- [ ] Golden tests: default = no prompt; full = redacted content
- [ ] Config documented in `docs/reference/config/observability.md`

**Effort:** 1–2 days

**DX Impact:** "Why is this slow/flaky" → spans/metrics immediately available.

---

### 8.4 Replay Bundle (DX + Forensic)

**Goal:** Reproducible test runs from a single artifact; supply-chain aware.

#### 8.4.1 Bundle Format

```
.assay/replay.bundle/
├── manifest.json          # Provenance + file digests + toolchain
├── config/
│   ├── eval.yaml
│   └── policy.yaml
├── traces/
│   └── input.jsonl
├── cassettes/             # VCR recordings (scrubbed)
│   └── openai/
│       └── *.json
├── baseline/
│   └── baseline.json
└── toolchain/             # NEW: for true reproducibility
    ├── Cargo.lock
    └── cargo-metadata.json
```

#### 8.4.2 Bleeding Edge: Toolchain Capture (Critical for Reproducibility)

Without toolchain capture, "replay works on my machine" is common. Include:

```json
{
  "schema_version": 2,
  "created_at": "2026-01-30T12:00:00Z",
  "assay_version": "2.12.0",
  "git_sha": "abc123...",
  "workflow_run_id": "12345678",
  "toolchain": {
    "rustc": "rustc 1.84.0 (9fc6b4312 2025-01-07)",
    "cargo": "cargo 1.84.0 (66221abde 2024-11-19)",
    "target_triple": "aarch64-apple-darwin",
    "cargo_lock_digest": "sha256:abc123...",
    "cargo_metadata_snapshot": "sha256:def456..."
  },
  "runner": {
    "os": "Linux",
    "os_version": "Ubuntu 22.04.3 LTS",
    "runner_image": "ubuntu-latest",
    "uname": "Linux 6.5.0-1025-azure x86_64"
  },
  "files": {
    "config/eval.yaml": { "sha256": "...", "size_bytes": 1234 },
    "traces/input.jsonl": { "sha256": "...", "size_bytes": 5678 }
  },
  "bundle_digest": "sha256:...",
  "tool_versions": {
    "openai_sdk": "1.x.x",
    "reqwest": "0.12.x"
  }
}
```

**Captured files:**
- `Cargo.lock` (exact dependency versions)
- `cargo metadata --format-version 1` snapshot
- `rustc -Vv` output
- Runner environment metadata

#### 8.4.3 Bleeding Edge: Deterministic Seed Logging

For judge reliability: seed is logged → replay with same seed = same order.

```json
{
  "determinism": {
    "judge_order_seed": 42,
    "random_seed": 12345,
    "timestamp_frozen": false
  }
}
```

| File | Change |
|------|--------|
| `crates/assay-core/src/replay/bundle.rs` | **New.** Bundle creation + manifest generation |
| `crates/assay-core/src/replay/manifest.rs` | **New.** Manifest schema + digest computation + toolchain capture |
| `crates/assay-cli/src/cli/commands/replay.rs` | **New.** `assay replay --bundle <path>` command |

#### 8.4.4 Bleeding Edge: Scrubbed Cassettes Policy

**SOTA:** Scrubbing **deny-by-default** (allowlist van toegestane velden, niet blocklist). Zo blijft bundle veilig bij nieuwe velden.

```yaml
replay:
  include_prompts: false        # default
  scrub_cassettes: true         # remove secrets from VCR cassettes
  scrub_policy: "default"       # allowlist (niet blocklist)
```

| File | Change |
|------|--------|
| `crates/assay-core/src/replay/scrub.rs` | **New.** Cassette scrubbing: **deny-by-default** (allowlist); geen magische blocklist. |
| Tests | Bundle is safe to share (no secrets, no PII). |

#### 8.4.5 Privacy: Minimal Secrets Risk

| Default | Behavior |
|---------|----------|
| `replay.include_prompts: false` | No prompt/response content in bundle unless explicit |
| `replay.include_cassettes: true` | VCR cassettes included (scrubbed) |
| `replay.scrub_cassettes: true` | Remove API keys, tokens, PII from cassettes |

#### 8.4.6 CLI Interface

```bash
# Create bundle from last run
assay bundle create --output replay.bundle

# Replay bundle (offline, VCR mode)
assay replay --bundle replay.bundle

# Replay with network (re-run against live providers)
assay replay --bundle replay.bundle --live

# Replay with specific seed (for judge order reproducibility)
assay replay --bundle replay.bundle --seed 42
```

#### 8.4.7 Definition of Done

- [ ] **Toolchain capture** (rustc, cargo, lock, metadata, runner)
- [ ] **Deterministic seed logging** for reproducibility
- [ ] Manifest with file digests + provenance
- [ ] `assay replay --bundle` reproduces (VCR, deterministic seeds)
- [ ] Scrubbed cassettes policy + tests
- [ ] Privacy: no prompts/secrets unless opt-in
- [ ] Signature placeholder (structure for later Sigstore/cosign)

**Effort:** 2–3 days

**DX Impact:** Reviewers can reproduce "exactly this" locally. Bundle is often the "next step" on failures.

---

### 8.5 P1 File-Level Checklist (Updated)

| File / Area | P1.3 MCP | P1.1 Judge | P1.2 OTel | Replay |
|-------------|----------|------------|-----------|--------|
| `crates/assay-mcp-server/src/auth/` | Resource + BCP + **alg/typ/crit** | — | — | — |
| `crates/assay-mcp-server/src/auth/jwks.rs` | JWKS rotation + cache + **stale-while-revalidate** | — | — | — |
| `crates/assay-mcp-server/src/auth/dpop.rs` | DPoP + **jti cache** | — | — | — |
| `crates/assay-core/src/judge/borderline.rs` | — | Borderline band | — | — |
| `crates/assay-core/src/judge/order.rs` | — | **Randomized order** (NEW) | — | — |
| `crates/assay-core/src/judge/reliability.rs` | — | Order-invariance | — | — |
| `crates/assay-core/src/judge/rerun.rs` | — | 2-of-3 + **instability triggers** | — | — |
| `crates/assay-core/src/judge/policy.rs` | — | Fail policies **per suite type** | — | — |
| `crates/assay-core/src/otel/genai.rs` | — | — | GenAI spans + **semconv version** | — |
| `crates/assay-core/src/otel/metrics.rs` | — | — | LLM metrics + **cardinality tests** | — |
| `crates/assay-core/src/otel/redaction.rs` | — | — | **Composable** redaction | — |
| `crates/assay-core/src/replay/bundle.rs` | — | — | — | Bundle create |
| `crates/assay-core/src/replay/manifest.rs` | — | — | — | Manifest + **toolchain** |
| `crates/assay-core/src/replay/scrub.rs` | — | — | — | **Cassette scrubbing** (NEW) |
| `crates/assay-cli/src/cli/commands/replay.rs` | — | — | — | CLI |
| `crates/assay-cli/src/exit_codes.rs` | — | E_JUDGE_UNCERTAIN | — | — |
| Tests (negative) | alg/typ/crit, JWKS, passthrough, **jti cache** | order-invariance, consensus, **instability** | redaction goldens, **cardinality** | bundle roundtrip, **scrubbed** |

---

### 8.6 P1 Effort Summary

| Epic | Effort | Dependencies |
|------|--------|--------------|
| P1.3 MCP Auth Hardening | 2–3 days | None (security baseline) |
| P1.1 Judge Reliability MVP | 2–3 days (+1 tuning) | P1.3 done |
| P1.2 OTel GenAI | 1–2 days | P1.1 helps with tuning |
| Replay Bundle | 2–3 days | All above (uses their outputs) |
| **Total** | **8–12 days** | Sequential with parallelization possible |

**DX-items priority:** #10 (next steps) → #11 (perf DX) → #13 (privacy) — highest impact first.

---

### 8.7 PR Sequence Blueprint

Recommended PR structure for implementation:

```
PR 1: P1.3 MCP Auth Hardening
  ├── auth/resource.rs (RFC 8707)
  ├── auth/jwt_validation.rs (alg/typ/crit)
  ├── auth/jwks.rs (cache improvements)
  ├── auth/dpop.rs (optional, behind feature flag)
  └── tests/auth_negative.rs

PR 2: P1.1 Judge Reliability
  ├── judge/borderline.rs
  ├── judge/order.rs (randomized default)
  ├── judge/rerun.rs (instability triggers)
  ├── judge/policy.rs (suite-type policies)
  └── tests/judge_reliability.rs

PR 3: P1.2 OTel GenAI
  ├── otel/genai.rs (semconv versioned)
  ├── otel/metrics.rs (low-cardinality)
  ├── otel/redaction.rs (composable)
  └── tests/otel_cardinality.rs

PR 4: Replay Bundle
  ├── replay/bundle.rs
  ├── replay/manifest.rs (toolchain, seeds)
  ├── replay/scrub.rs
  └── tests/bundle_roundtrip.rs

DX Mini-PRs (parallel):
  ├── #10: suggest_next_steps()
  ├── #11: slowest 5 + phase timings
  └── #13: privacy defaults + redaction tests
```

---

## 9. References

- **§0 Epics Overview** — epics E1–E9 met stories, acceptance criteria en effort
- [DX-REVIEW-MATERIALS.md](DX-REVIEW-MATERIALS.md) — current DX review materials
- [ADR-019 PR Gate 2026 SOTA](architecture/ADR-019-PR-Gate-2026-SOTA.md) — performance, DX, security, judge, observability
- [ROADMAP](ROADMAP.md) — strategic roadmap
- [reference/cli/run.md](reference/cli/run.md) — run exit codes and outputs
- [guides/troubleshooting.md](guides/troubleshooting.md) — troubleshooting guide
