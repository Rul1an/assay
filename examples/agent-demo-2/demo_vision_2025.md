# Demo DX Vision 2025 (Assay)

## Why this matters
A dev tool for LLM apps wins on:
1) the first 5 minutes (wow / clarity)
2) the next 5 days (frictionless iteration)
3) the next 5 months (observability + governance)

Assay already has a strong core (streaming traces, SQLite, assertions, OTel ingest).
DX is how we make it *feel* inevitable.

---

## Immediate “SOTA” demo goals (today)
### 1) One command, one story
- `python3 demo_tui.py all`
- Record traces (mock / offline)
- Ingest + replay-strict
- Assertions gate
- A scoreboard + suggested next actions

### 2) Live UI feedback
- Stage progress (record → ingest → replay → assertions)
- Live log tail panel
- Friendly error surfacing (Trace miss, prompt mismatch, assertion failures)
- Print “closest match” hints (already present in Assay output)

### 3) Determinism-by-default
- No live calls in the demo
- Trace file is the source of truth for replay
- SQLite is the source of truth for assertions/queries

---

## Next-level DX (weeks)
### A) `assay doctor`
A single command that checks:
- sqlite schema compatibility
- trace file format + upgrade path
- config version compatibility
- replay readiness (are prompts matchable?)
- suggests exact commands to fix issues

### B) `assay report --format summary`
Developer-facing report:
- pass rate by suite
- first failing step
- p95 tool calls
- top assertion failures

### C) “Red Button” playground
`assay playground`
- Pick a test
- Run once live (if allowed)
- Save trace + baseline
- Re-run offline and compare
- Export GitHub Actions snippet

---

## Observability direction (months)
### 1) OTel-first
- ingest spans → episodes/steps/toolcalls
- keep full provenance of truncations (already supported)
- consistent mapping rules and docs

### 2) Explainable gating
- “why did CI fail?” should be 1 screen:
  - which assertion failed
  - first failing step + tool call details
  - closest prompt matches / suggestion to fix mismatches

### 3) Baselines + drift
- export baseline on main
- compare in PR
- optional calibrate for confidence / stability

---

## Design principles
- No hidden state: what’s replayed is what’s gated
- Idempotent ingest: safe to run twice
- “Correctness first, convenience second” (but add convenience that preserves correctness)
- Make the happy path *obvious* and the failure path *actionable*
