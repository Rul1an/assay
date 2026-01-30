# ADR-019: PR Gate 2026 SOTA — Implementation Plan v1

**Status:** Proposed
**Date:** 2026-01
**Extends:** ADR-004 (exit code 3, judge strategy); complements ADR-017 (main store only; ADR-017 covers MandateStore WAL).

**Related:** [ROADMAP](../ROADMAP.md), [DX-IMPLEMENTATION-PLAN](../DX-IMPLEMENTATION-PLAN.md), [ADR-003 Gate Semantics](./ADR-003-Gate-Semantics.md), [ADR-014 GitHub Action v2](./ADR-014-GitHub-Action-v2.md), [ADR-018 GitHub Action v2.1](./ADR-018-GitHub-Action-v2.1.md)

---

## Context

Assay's PR gate must be the default choice for teams — not something they disable when it gets in the way. This ADR chooses the highest-ROI, realistic measures (1–2 quarters) and aligns them with best practice and bleeding-edge research/publications as of January 2026.

### What “best practice / SOTA” means (Jan 2026)

PR-gate tooling wins only if it:

1. **Gives PR-native feedback** — JUnit + SARIF in the PR (no “rapportje”); respects GitHub limits (upload size, result count) so uploads never fail randomly.
2. **Stays predictable despite non-determinism** — Judge reliability varies per instance; bias and class-imbalance are real. Consensus alone is not enough; variance-aware handling is required.
3. **Is secure by default** — Especially around MCP auth: resource indicators (RFC 8707) and no token pass-through are hard requirements.
4. **Has observability without leaking privacy** — OTel GenAI conventions are the direction; GenAI *events* (prompt/response capture) are still in development in many stacks → content capture is opt-in.

### North Star

A PR gate that teams do not turn off because it is:

- **Fast enough** — warm cache feels “free”; no tail latency from store contention.
- **Secure by default** — no accidental disable of verification; MCP and audit posture without footguns.
- **Predictable** — low flake rate, clear reasons when something fails, variance handled explicitly.
- **Native in CI** — JUnit, SARIF, Check Run Summary; stable exit codes and reason codes; no custom glue.

### Scope choice: highest ROI, lowest risk

**We do (highest ROI / realistic in 1–2 quarters):**

1. **PR-native Eval Diff UX (no SaaS)** — Checks/SARIF/JUnit + smart truncation for GitHub limits.
2. **Blessed flow + contracts** — One entrypoint (`assay ci`), stable exit codes + reason codes.
3. **Store performance** — WAL + single-writer batching + backpressure (stability & tail latency).
4. **Judge reliability MVP** — Variance-aware “borderline rerun” + “uncertain” handling (no statistics project).
5. **MCP auth hardening** — Resource parameter + no pass-through + negative tests.

**We park (valuable, but scope risk):**

- **Full supply-chain attestations** (SLSA/in-toto) for every run: only after DX/PR-gate is solid. A **lightweight replay bundle** (see §5) is in scope as a stepping stone.

---

## Decision

### P0 — Must-have (directly better DX + reliability)

#### P0.1 PR-native Eval Diff UX (highest ROI)

**Goal:** In a PR, users see immediately “what got worse” without an external viewer.

**Decisions:**

- **SARIF for core findings only (compact).** GitHub has hard limits (e.g. max 10MB gzip, max results); uploads that exceed them are rejected. SARIF MUST stay within limits: truncate to top N results + “N omitted” message so upload never fails on size.
- **Check Run Summary (GitHub step summary)** carries the “diff”: top regressions, score deltas, short snippets, links to `assay explain` per finding.
- SARIF results MUST include at least one location per result (synthetic if needed) for GitHub `upload-sarif` compatibility; contract tests validate this.

**Acceptance criteria:**

- SARIF upload never fails due to size/limits: truncate to top N + “N omitted”.
- In a PR: (1) top regressions visible, (2) “reproduce locally” link, (3) link to explain per finding.

**Why this beats comparables:** Tools like promptfoo use PR comments + viewer link; Assay offers the same speed with deeper native integration (Security tab + Tests + Summary) without SaaS lock-in.

---

#### P0.2 One blessed flow + contracts (DX foundation)

**Goal:** Zero confusion between run vs ci vs action variants.

**Decisions:**

- **`assay ci` = blessed entrypoint.** Always the same outputs: `junit.xml`, `sarif.json`, `summary.json`. **summary.json MUST include schema_version** for compatibility.
- **Exit codes stay coarse (0/1/2/3).** Introduce **stable reason codes** in summary.json and console (e.g. E_TRACE_NOT_FOUND, E_JUDGE_UNAVAILABLE, E_CFG_PARSE) so behaviour is machine-readable without breaking exit-code semantics. Avoid redefining exit 3 in a breaking way; use reason codes for nuance.
- **First 15 minutes:** `assay init --ci github` generates a workflow that works out of the box and is up-to-date (blessed action v2).
- **Every failure ends with one next step** — e.g. “Run: assay doctor …”, “See: assay explain …”, “Fix baseline: …”.

**Acceptance criteria:**

- “First 15 minutes”: `assay init --ci github` produces a workflow that runs successfully.
- Every non-zero exit has a stable reason code and one suggested next step in console (and in summary.json where applicable).

---

#### P0.3 Store performance: WAL + single-writer batching + bounded queue

**Goal:** No tail latency and no lock contention under parallel runs.

**Scope:** Main assay-core Store (run/results/embeddings), not MandateStore (ADR-017).

**Decisions:**

- **WAL + pragmas:** Enable `journal_mode=WAL`, `synchronous=NORMAL` (document durability trade-off), configurable `busy_timeout`, and `wal_autocheckpoint` (configurable) to avoid WAL growth and checkpoint spikes. Document default vs tunable pragmas.
- **Writer transactions:** Writer MUST use `BEGIN IMMEDIATE` (not DEFERRED) to avoid SQLITE_BUSY.
- **Single writer queue:** One async writer; batched commits (e.g. every N ops or X ms). **Bounded capacity** with backpressure (producer blocks when full). **Deterministic shutdown flush** so in-flight writes are not lost.
- **Reduce chattiness:** Batch inserts per transaction.
- **Indices:** Add/verify indices on hot dimensions (suite_id, run_id, test_id, status, timestamp).
- **Metrics/bench:** store_write_ms, store_wait_ms, txn_batch_size, sqlite_busy_count, p95_test_duration_ms. “Standard concurrency configuration” (e.g. 4 workers, single writer, no external writers) is documented so “sqlite_busy_count == 0” is unambiguous.

**Acceptance criteria:**

- Warm run: p95 per-test duration improves by at least 30% on large traces.
- sqlite_busy_count == 0 under standard concurrency configuration.
- Throughput: at least 5k inserts/sec sustained in a synthetic benchmark (no tail spikes/locks).

**Status: opgelost (met scope)** — Zie [PERFORMANCE-ASSESSMENT](../PERFORMANCE-ASSESSMENT.md). Voor de **huidige worstcase workload + parallelmatrix** (zoals gemeten) is P0.3 opgelost: batching (insert_results_batch aan het einde van de run) + BEGIN IMMEDIATE + busy handler; store_wait_ms (parallel 16) daalde van 27→3 ms (median), 28→5 ms (p95); wall p95 van 50→34 ms. **Scope:** Opgelost voor deze workload; niet universeel bewezen voor andere workloads (grotere payloads, meerdere readers, CI filesystem jitter). **Writer-queue + bounded channel** blijft als contingency/“next level” voor wanneer store_wait_ms weer oploopt, meer write-paths bijkomen, of meerdere DB consumers (bijv. background ingest / parallel suites). Gebruik dan een **bounded** mpsc (backpressure); unbounded is een perf/memory footgun.

---

#### P0.4 Security footguns closed: --no-verify + defaults

**Goal:** Teams cannot accidentally disable security.

**Decisions:**

- **--no-verify:** Explicitly UNSAFE: show a banner and “UNSAFE: signature verification disabled”. In CI, **--no-verify fails unless explicitly allowlisted** (e.g. env var or workflow input). Mark artifacts (e.g. summary.json) with verify_mode: disabled.
- **Secure defaults:** allow_embedded_key: false by default; deny-by-default for write/commit tools in trust policy.
- **Artifact provenance:** Every artifact MUST include: verification status, key_id (when applicable), policy hash (when applicable), **assay_version**, **policy_pack_digest**, **baseline_digest**, **trace_digest** (optional), **verify_mode**. Document log redaction defaults (no prompt/response in logs by default).

**Acceptance criteria:**

- In CI (e.g. GHA), `--no-verify` is impossible unless explicitly allowlisted.
- Every artifact includes provenance: verification status, assay_version, policy_pack_digest, baseline_digest, verify_mode; trace_digest optional.

---

### P1 — SOTA differentiators (no scope explosion)

#### P1.1 Judge reliability (MVP that works)

**Context:** Research shows judge reliability varies per instance; consensus/ensemble helps but bias and class-imbalance can overstate reliability.

**Decisions:**

- **Deterministic first;** use judge only where needed.
- **“Borderline band”** → only then trigger 3× rerun (temperature=0, pinned model).
- **Output:** consensus_rate, variance, judge_failures (so CI/summary can show judge health).
- **Handling policy:**
  - **Security suites:** fail-closed.
  - **Quality suites:** “uncertain” with warning + optional human review (configurable).

**Acceptance criteria:**

- **“Same trace and config”** is defined (same trace file, eval config, model/judge revision, seed where applicable); document for 20-run consistency.
- Same trace and config over 20 runs: outcome is ≥99% consistent (same PASS/FAIL) or explicitly “uncertain” with predictable handling.
- Calibration suite can detect drift on model/judge upgrade.

---

#### P1.2 OTel GenAI: spans/metrics default, events opt-in

**Context:** OTel GenAI semconv is the direction; GenAI *events* (prompt/response capture) are still in development and not everywhere → privacy-safe default.

**Decisions:**

- **Default export:** Spans + metrics (latency, tokens, cache hits). **Spans and metrics are required.**
- **Prompt/response events:** Opt-in only; redaction policies must be testable.
- **Replay/debug:** Possible from traces/metadata without exporting prompt content.

**Acceptance criteria:**

- A run can be replayed from OTel export (no provider lock-in) without leaking prompts.
- Redaction policies (e.g. PII/secrets) are tested before export.

---

#### P1.3 MCP auth hardening

**Context:** MCP spec requires resource indicators and forbids token pass-through; non-compliance is a token-misuse class vulnerability.

**Decisions:**

- **Client:** Use resource parameter (RFC 8707) when requesting tokens.
- **Proxy/server:** Validate issuer/audience/resource; **no pass-through** — downstream gets its own tokens. Tool scopes tied to resource/audience.
- **Spec pinning:** Pin MCP auth spec version/URL so implementations do not drift.
- **Negative tests:** Token for resource A does not work for resource B; reject token without resource param; reject wrong issuer/aud; replay/expired/clock skew covered.

**Acceptance criteria:**

- Token misuse regressions are caught by tests (negative test suite).

---

### 5. Replay Bundle (lightweight differentiator)

**Goal:** Support and DX win without turning into a “full provenance platform”.

**MVP:**

- **Artifact:** `.assay/replay.bundle` containing:
  - Config/policy/baseline digests
  - Input traces (or pointer + digest)
  - Outputs (junit/sarif/summary)
  - Environment metadata (assay version)
- **Command:** `assay replay --bundle <path>` — best-effort deterministic; for judge, record/replay of outputs is optional.
- **PR summary:** Can always offer “Reproduce locally” using the replay bundle.

**Why ROI is high:** Support: “send bundle” → reproduce exactly, less back-and-forth. DX: one-click “reproduce locally” from PR.

---

## Risks and mitigations

| Risk | Mitigation |
|------|------------|
| **SARIF limits** | Ignoring GitHub limits causes random upload failures on larger repos → **truncation + compact results is P0** (P0.1). |
| **Judge cost/variance** | Reruns only on borderline band; otherwise CI time/cost explodes. Mitigate bias via “minority veto / uncertain” instead of blind majority (P1.1). |
| **OTel privacy** | Events opt-in; otherwise prompt-leak risk. Spec itself notes events are in development → **events opt-in** (P1.2). |
| **MCP auth** | Spec compliance is mandatory; otherwise token misuse vulnerabilities → **resource + no pass-through + negative tests** (P1.3). |

---

## Rollout plan (minimum)

- **P0.3 (Store):** Implement behind a feature flag; measure in CI (store metrics, sqlite_busy_count, p95); enable by default only after acceptance criteria and no regressions.
- **Other P0/P1:** Ship when acceptance and contract tests pass; document migration impact (Compatibility).
- **Replay bundle:** Ship when MVP artifact + `assay replay --bundle` meet definition above; document in CLI and CI docs.

---

## Compatibility

- **Output schema versioning:** summary.json (and other stable outputs) MUST carry a schema_version; document version history and migration so CI consumers can detect and adapt.
- **Migration impact:** Document impact for existing CI users (exit code 3, reason codes, new artifact fields, SARIF location/truncation); provide migration notes or a compatibility window where old behaviour is deprecated but still supported where feasible.
- **DX implementation:** Concrete per-file changes and test cases (init template v2, exit/reason codes, SARIF locations/truncation, JUnit/snippets, fork fallback, etc.) are in [DX-IMPLEMENTATION-PLAN.md](../DX-IMPLEMENTATION-PLAN.md).
- **Specifications:** Normative output and replay contracts are in:
  - [SPEC-PR-Gate-Outputs-v1](./SPEC-PR-Gate-Outputs-v1.md) — summary.json schema, exit/reason code registry, SARIF location and truncation rules, next-step requirement.
  - [SPEC-Replay-Bundle-v1](./SPEC-Replay-Bundle-v1.md) — replay bundle format, manifest schema, `assay replay --bundle` semantics.

---

## Consequences

- **Easier:** PR-native diff UX, single blessed path, predictable exit/reason codes, safer defaults, less store contention, judge variance handled, replay bundle for support/DX.
- **Harder:** Writer queue (bounds, backpressure, flush), SARIF truncation and contract tests, reason-code registry, judge borderline/uncertain logic, MCP/OTel and redaction; replay bundle format and replay semantics.
- **Test strategy:** Contract tests for SARIF (schema + “at least one location” + upload-smoke); negative tests for MCP auth; redaction tests; bench harness for store; optional 20-run consistency suite for judge.
- **Definition of done:** Each work package is done when a PR is merged with an acceptance check that demonstrates completion.

---

## Relations to existing ADRs

| ADR | Relation |
|-----|----------|
| ADR-003 | Kept; ADR-019 adds blessed flow and strict exit/reason codes. |
| ADR-004 | Extended: exit code 3, judge strategy with borderline rerun and uncertain handling (decisions here; ADR-004 can note “Extended by ADR-019”). |
| ADR-011 | Kept; ADR-019 adds MCP resource indicators and no pass-through. |
| ADR-014 / ADR-018 | Kept; ADR-019 anchors SARIF/JUnit contract, truncation, and one blessed flow. |
| ADR-017 | Unchanged; WAL remains for MandateStore; ADR-019 applies only to the main run store. |

---

## Appendix: Backlog (copy-paste for issue tracking)

### P0

1. **PR-native Eval Diff UX:** SARIF truncate to top N + “N omitted”; at least one location per result; contract tests (schema + upload-smoke). Check Run Summary: top regressions, “reproduce locally”, links to explain.
2. **Blessed flow + contracts:** assay ci with junit.xml, sarif.json, summary.json (schema_version); reason codes in summary.json and console; init --ci github generates working v2 workflow; every failure suggests one next step.
3. **Store:** WAL + busy_timeout + wal_autocheckpoint; BEGIN IMMEDIATE; single-writer queue (bounded, backpressure, flush-on-drop); batched transactions; indices; metrics and bench; “standard concurrency” documented.
4. **Security:** --no-verify explicit UNSAFE + CI allowlist; artifact provenance (assay_version, policy_pack_digest, baseline_digest, verify_mode); log redaction defaults documented.
5. Rollout: Store behind feature flag; compatibility notes and output schema versioning.

### P1

6. **Judge reliability MVP:** Borderline band → 3× rerun (temp=0, pinned model); output consensus_rate, variance, judge_failures; security = fail-closed, quality = uncertain + warning; “same trace/config” defined; 20-run consistency or “uncertain”.
7. **OTel GenAI:** Spans + metrics default; prompt/response events opt-in; redaction tests; replay without leaking prompts.
8. **MCP auth:** Resource (RFC 8707), no pass-through, spec pinned; negative tests (wrong resource, missing resource, wrong issuer/aud, replay/expired/clock skew).
9. **Replay bundle:** .assay/replay.bundle format (digests, traces, outputs, env); assay replay --bundle; document “Reproduce locally” in PR summary.

An acceptance test matrix (expected metrics and outputs per deliverable) can be maintained in a separate document or issues; it is not part of this ADR.
