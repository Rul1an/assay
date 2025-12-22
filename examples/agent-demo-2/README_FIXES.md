# Agent Demo 2: Quickstart

This demo showcases **Verdict's** ability to gate dangerous agent behaviors using trace analysis.

## Status: 🟩 Operational
*   **Infrastructure**: Fully working (Ingest, Replay, Assertions).
*   **Results**: ~50% Pass / ~50% Fail.
    *   **Failures are expected!** The demo agent is intentionally "unsafe" (it tries to apply discounts without permission) and "lazy" (it skips some tool calls).
    *   **Verdict's Job**: The red crosses prove that Verdict is **catching** these violations.

## How to Run

1.  **Generate Config**:
    ```bash
    python3 scenarios.py --yaml > verdict.yaml
    ```

2.  **Record Traces** (Simulates agent traffic):
    ```bash
    OPENAI_API_KEY=mock python3 run_demo.py record
    ```

3.  **Verify** (Run Verdict CI Policy):
    ```bash
    OPENAI_API_KEY=mock python3 run_demo.py verify
    ```

## Key Fixes Applied
*   **Data Integrity**: Fixed DB collisions where tests overwrote each other's steps.
*   **Robustness**: Fallback logic now handles missing links gracefully.
*   **Determinism**: Strict replay now enforces exact prompt matching without truncation errors.
