# Agent Demo 2: Integration Fixes & Technical Summary

This document serves as the canonical reference for the fixes applied to integrate Agent Demo 2 with `verdict-core` (v0.4.0+).

## Goal
Stabilize the end-to-end flow for Agent Demo 2:
- **Consistent Ingestion**: Parse V2 JSONL traces into SQLite without data loss.
- **Deterministic Replay**: Enforce strict matching against recorded traces (`--replay-strict`).
- **Reliable Assertions**: Ensure tests pass/fail based on logic, not infrastructure errors.

---

## 1. Verdict Core Hardening (`store.rs`)

### A. Persistent Linking on Upsert (`insert_episode`)
**Problem:** Re-ingesting a trace (e.g., to update metadata) without explicit `run_id` or `test_id` in the payload would overwrite existing database links with `NULL`, creating "orphan" episodes.
**Fix:** Updated `ON CONFLICT` clause to use `COALESCE`:
```sql
run_id = COALESCE(excluded.run_id, episodes.run_id)
test_id = COALESCE(excluded.test_id, episodes.test_id)
```
**Result:** Links are preserved across updates; data is safe by default.

### B. Default Test ID Fallback
**Problem:** Many traces (e.g., OTel exports) lack an explicit `test_id`, preventing the fallback query (`get_latest_episode_graph_by_test_id`) from finding them.
**Fix:** In `insert_episode`, default `test_id` to `episode_id` if not provided.
**Result:** Every episode is indexable and retrievable by its ID.

### C. Step ID Primary Key Collision
**Problem:** Re-ingesting steps failed with `UNIQUE constraint failed: steps.id` because the upsert logic only checked the logical key `(episode_id, idx)`, not the primary key `id`.
**Fix:** Updated `insert_step` to handle conflicts on `id`:
```sql
ON CONFLICT(id) DO UPDATE SET content=excluded.content, ...
```
**Result:** Step ingestion is idempotent and robust.

### D. Fallback Query Schema Update
**Problem:** The fallback query used legacy column names, crashing on V2 schemas.
**Fix:** Rewrote `get_latest_episode_graph_by_test_id` to match the exact V2 schema and join logic of `get_episode_graph`.
**Result:** Reliable fallback lookup when strict trace linking isn't available.

---

## 2. Demo Script Fixes

### A. Unique Step Generation (`agent.py`)
**Problem:** The mock agent generated generic step IDs (`step_001`, `step_002`) for every episode. Since `steps.id` is a primary key, running multiple tests caused data collisions (later tests overwrote earlier ones).
**Fix:** Prefixed step IDs with the episode ID (e.g., `weather_simple_step_001`).
**Result:** All 25 scenarios persist correctly in the database without collision.

### B. Strict Replay Configuration (`run_demo.py`)
**Problem:** `verdict ci` in strict mode failed because it attempts to use a live LLM client if no trace source is provided (which is forbidden).
**Fix:** Explicitly pass `--trace-file traces/recorded.jsonl` in the verification command.
**Result:** The runner correctly initializes the `TraceClient` for offline replay.

### C. Prompt Fidelity (`scenarios.py`)
**Problem:** The script truncated prompts to 80 characters (`...`) in `verdict.yaml` for readability. This caused a content mismatch with the full recorded traces, leading to `Trace miss` errors.
**Fix:** Removed truncation logic; `verdict.yaml` now contains exact prompts.
**Result:** Prompts match 100%, ensuring successful deterministic replay.

---

## 3. Status & Usage

### Current State
*   **Infrastructure**: ✅ Green (Ingest + Replay + Verification working).
*   **Test Results**: 13/25 Passed.
    *   **Failures (12/25)**: Correctly failing assertions. The Mock Agent is simple and does not perform all requested actions (e.g., unit conversion), so `trace_must_call` assertions fail.
    *   **Safety**: Dangerous actions (like `ApplyDiscount`) are correctly blocked and detected.

### How to Run
```bash
# 1. Record traces (Mock Mode)
python3 scenarios.py --yaml > verdict.yaml
OPENAI_API_KEY=mock python3 run_demo.py record

# 2. Verify (Ingest + Check)
OPENAI_API_KEY=mock python3 run_demo.py verify
```

This setup proves that **Verdict** is working correctly as a safety guardrail.
