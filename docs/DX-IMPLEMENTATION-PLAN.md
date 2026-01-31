# DX Implementation Plan — Default Gate Readiness

**Status:** Draft
**Date:** 2026-01
**Source:** Critical DX review of [DX-REVIEW-MATERIALS.md](DX-REVIEW-MATERIALS.md); aligns with [ADR-019 PR Gate 2026 SOTA](architecture/ADR-019-PR-Gate-2026-SOTA.md) and [ROADMAP](ROADMAP.md).

This document turns the DX review into a concrete backlog with **per-file patchlist** and test cases. Work is ordered P0 (must-have before default gate) then P1 (SOTA).

---

## 1. First 15 minutes: init as blessed on-ramp

### 1.1 Template drift (v1 → v2 action in init --ci)

**Problem:** `assay init --ci` (and `assay init-ci --provider github`) generate a workflow that uses `assay-action@v1` and `assay_version: "v1.4.0"`, while the recommended and documented action is `assay-action@v2`. Trust break in minute 5.

**Fix:** Init-generated GitHub workflow MUST use the blessed v2 template (semver range or exact pin + changelog notice in docs).

| File | Change |
|------|--------|
| `crates/assay-cli/src/templates.rs` | Replace `CI_WORKFLOW_YML`: `uses: Rul1an/assay-action@v1` → `uses: Rul1an/assay/assay-action@v2`; remove or replace `assay_version: "v1.4.0"` with a semver range (e.g. `version: "2.x"` or exact `"2.x.y"`) and add a short comment in template: "Update to latest v2: see CHANGELOG." |
| `docs/getting-started/ci-integration.md` (or equivalent) | Add one line: "assay init --ci generates workflow with assay-action@v2; pin to 2.x or exact release. See CHANGELOG for releases." |
| `docs/reference/cli/init.md` | State that init --ci / init-ci github outputs the **blessed** workflow (assay-action@v2). |

**Test cases:**

- `assay init --ci` in empty dir → `.github/workflows/assay.yml` contains `assay-action@v2` (or `assay/assay-action@v2`) and no v1 reference.
- `assay init-ci --provider github` → same.
- Optional: golden snapshot of `CI_WORKFLOW_YML` in tests (e.g. `tests/fixtures/contract/` or assay-cli test).

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

**Fix:** Action runs with JUnit by default (or documented default); one blessed snippet in docs.

| File | Change |
|------|--------|
| `assay-action/action.yml` | Ensure the step that runs assay uses `--output junit` by default (or add input `junit: true` default true), and writes to a known path (e.g. `.assay/reports/junit.xml`). Add upload of JUnit artifact and, if applicable, use a well-known JUnit reporter action (e.g. EnricoMi/publish-unit-test-result-action or similar) so failures show as annotations. |
| `docs/reference/cli/run.md` | In "JUnit (CI Test Results)", add subsection **"Failures as annotations"**: one blessed YAML snippet showing assay run with `--output junit`, then upload artifact + JUnit report action so PR shows annotations. Add **"Where is junit.xml"**: default path `.assay/reports/junit.xml` (or `--junit` override). |
| `docs/DX-REVIEW-MATERIALS.md` | B.1: "Action default: --output junit; blessed snippet in run.md." |

**Test cases:**

- CI workflow using the blessed snippet from run.md produces JUnit artifact and annotations on failure (manual or e2e).

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

**Fix:**

| File | Change |
|------|--------|
| `assay-action/action.yml` | Already conditional on same-repo for SARIF/comment. Make explicit in comments/docs: fork PRs = no SARIF upload, no PR comment (permissions). Ensure job summary (GitHub step summary) is always written so fork PRs still see results there. |
| `docs/DX-REVIEW-MATERIALS.md` or CI docs | Add: "Fork PRs: SARIF upload and PR comment are skipped (GitHub permissions). Use job summary for results." |
| `docs/getting-started/ci-integration.md` | One sentence: "On fork PRs, only the job summary is updated; SARIF and PR comment require same-repo." |

**Test cases:**

- Documented behaviour; optional: trigger from fork and assert no upload/comment, summary present.

---

## 3. Exit codes: remove DX landmine (P0)

**Problem:** run.md says exit 3 = "Trace file not found"; ADR-019 wants 3 = "infra/judge unavailable". Redefining 3 breaks existing users/CI.

**Fix (SOTA):** Introduce a stable, machine-readable **error code registry** (decoupled from exit code). Keep exit codes coarse (0/1/2/3); make reason codes in summary.json and console the source of truth.

| File | Change |
|------|--------|
| `crates/assay-cli/src/cli/commands/mod.rs` (or new `error_codes.rs`) | Define error code constants: e.g. `E_TRACE_NOT_FOUND`, `E_JUDGE_UNAVAILABLE`, `E_CFG_PARSE`, etc. (registry). Map to exit: e.g. trace not found → 2 (config/user error) or keep 3 for trace-not-found during transition; judge unavailable → 3. Document in ADR-019: 3 = infra/judge unavailable; trace not found = 2 with code E_TRACE_NOT_FOUND. |
| `docs/architecture/ADR-019-PR-Gate-2026-SOTA.md` | In Compatibility: "Exit code 3: redefined from 'trace not found' to 'infra/judge unavailable'. Trace-not-found becomes exit 2 with reason code E_TRACE_NOT_FOUND. Deprecation: support --exit-codes=v1 (old 3=trace not found) for N releases or document migration window." |
| `docs/reference/cli/run.md` | Update Exit Codes table: 0/1/2/3 with new semantics; add "Reason codes" pointing to error code registry (summary.json + console). If deprecation: "Legacy: exit 3 was previously 'trace file not found'; use summary.json reason code for stable behaviour." |
| `docs/guides/troubleshooting.md` | Align with new exit codes; add "Trace file not found" under Exit 2 (or legacy note) and "Judge/infra unavailable" under Exit 3. |
| Summary.json / report pipeline | Ensure every non-zero exit includes a stable `reason_code` (and optional `message`) so CI can branch on reason, not only exit. |

**Test cases:**

- Run with missing trace → exit 2, reason_code E_TRACE_NOT_FOUND (or legacy 3 if --exit-codes=v1).
- Run with judge unavailable (mock) → exit 3, reason_code E_JUDGE_UNAVAILABLE.
- run.md and troubleshooting.md match behaviour.

---

## 4. Ergonomie & debuggability

### 4.1 Default "next step" in every error (P1)

**Problem:** Not every exit≠0 ends with 1–2 concrete commands.

**Fix:**

| File | Change |
|------|--------|
| `crates/assay-cli` (run/ci/doctor paths) | On non-zero exit, append 1–2 lines when possible: e.g. "Run: assay doctor --config ...", "See: assay explain ...", "Fix baseline: assay baseline record ...". Centralise in a small helper (e.g. `suggest_next_steps(exit_code, reason_code, context)`) and call from run/ci/doctor. |
| `docs/guides/troubleshooting.md` | Add short "Next steps" per error type (already partially there); ensure each section ends with a concrete command. |

**Test cases:**

- Trigger config error, missing trace, failing test; stdout contains at least one suggested command (assay doctor / explain / baseline).

---

### 4.2 Performance-DX: slowest 5, cache hit rate, phase timings (P1)

**Problem:** No "slowest 5 tests", "cache hit rate", or "total time per phase" in console or summary.

**Fix:**

| File | Change |
|------|--------|
| `crates/assay-core/src/report/console.rs` (and summary pipeline) | After run, compute: (1) slowest 5 tests (by duration_ms), (2) cache hit rate (e.g. skipped/total or from store), (3) phase timings (ingest/store/judge/report if available). Print in console summary and add to summary.json (schema_version already required by ADR-019). |
| `docs/reference/cli/run.md` or report docs | Document new summary fields: slowest_tests[], cache_hit_rate, phase_timings (or equivalent). |

**Test cases:**

- Run suite with multiple tests; summary.json contains slowest_tests (up to 5), cache_hit_rate, and phase timings; console shows them.

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

**Problem:** GenAI events (prompt/response capture) are not everywhere; default should not export prompt/response content.

**Fix:**

| File | Change |
|------|--------|
| `docs/architecture/ADR-019-PR-Gate-2026-SOTA.md` | Already says: default no prompt/response content export; spans/metrics required, events best-effort. |
| CLI / config | Expose "do-not-store-prompts" (or equivalent) in config/CLI; default on. Document in run/reference. |
| Tests | Add redaction tests: export with default settings does not contain prompt/response content (or only hashes/digests). |

**Test cases:**

- Redaction test: OTel export (or equivalent) with default config has no prompt/response body; optional digest/hash only.

---

## 6. Backlog summary (copy-paste for issues)

### P0 (must-have before default gate)

1. **Template v2:** `templates.rs` CI_WORKFLOW_YML → assay-action@v2, semver pin; docs init/ci-integration align.
2. **Blessed entrypoint:** Document init --ci as blessed, init-ci as alias (docs only).
3. **SARIF locations:** assay-core (and assay-evidence if applicable) guarantee ≥1 location per result; synthetic if needed.
4. **SARIF contract test:** Snapshot + schema + optional upload smoke for SARIF output.
5. **Exit code 3 + registry:** Error code registry (E_TRACE_NOT_FOUND, E_JUDGE_UNAVAILABLE, E_CFG_PARSE); exit 3 = infra/judge; trace not found → 2 + reason code; deprecation plan (--exit-codes=v1 or migration window); run.md + troubleshooting.md + summary.json reason_code.
6. **JUnit default + snippet:** Action default --output junit (or equivalent); run.md blessed snippet "failures as annotations" + "where is junit.xml".

### P1 (SOTA)

7. **DX demo repos:** examples/dx-demo-node, examples/dx-demo-python (minimal app, 1 test, workflow, baseline flow, README).
8. **Fork PR fallback:** Docs: fork = job summary only; action already conditional; document clearly.
9. **SARIF limits:** Truncate + "N results omitted" when over GitHub limits; configurable.
10. **Next step in errors:** suggest_next_steps() in run/ci/doctor; troubleshooting.md per-error next steps.
11. **Performance DX:** slowest 5, cache hit rate, phase timings in console + summary.json.
12. **Progress:** N/M tests, optional ETA in console.
13. **Privacy:** do-not-store-prompts default, redaction tests.

---

## 7. File-level checklist (patchlist)

| File / area | P0 | P1 |
|-------------|----|----|
| `crates/assay-cli/src/templates.rs` | v2 template, semver pin | — |
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
| Output | **Seed logged in summary.json** for replay |

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

- [ ] **Randomized order default** with seed in summary.json
- [ ] **Rerun-on-instability** (borderline + low_margin + order_flip + high_variance)
- [ ] Config-first policies per suite type (security/quality/regression)
- [ ] CI-run produces `consensus_rate`, `flip_rate`, `abstain_rate`, `margin`
- [ ] Reason codes `E_JUDGE_UNCERTAIN`, `E_JUDGE_UNAVAILABLE`
- [ ] Multi-judge config placeholder (structure, not full implementation)

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
| Tests | **New.** `test_metric_labels_bounded()` — assert label cardinality < MAX_ALLOWED, no dynamic strings |

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

```yaml
replay:
  include_prompts: false        # default
  scrub_cassettes: true         # remove secrets from VCR cassettes
  scrub_policy: "default"       # or custom regex list
```

| File | Change |
|------|--------|
| `crates/assay-core/src/replay/scrub.rs` | **New.** Cassette scrubbing policies |
| Tests | Bundle is safe to share (no secrets, no PII) |

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

- [DX-REVIEW-MATERIALS.md](DX-REVIEW-MATERIALS.md) — current DX review materials
- [ADR-019 PR Gate 2026 SOTA](architecture/ADR-019-PR-Gate-2026-SOTA.md) — performance, DX, security, judge, observability
- [ROADMAP](ROADMAP.md) — strategic roadmap
- [reference/cli/run.md](reference/cli/run.md) — run exit codes and outputs
- [guides/troubleshooting.md](guides/troubleshooting.md) — troubleshooting guide
