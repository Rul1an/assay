# Python SDK PR Checklist & Release Gates

Specific requirements for `verdict-sdk` (Python) to adhere to the core [DX Contract](DX_CONTRACT.md).

## 1. API & Backwards Compatibility
- [ ] **Public API**: No breaking changes in `verdict_sdk.*` imports or signatures without major version bump.
- [ ] **Defaults**: Defaults remain stable (e.g., `temperature=0.0`, `max_tool_rounds=4`).
- [ ] **Deprecations**: Changes include compat layer or clear error + migration note.

## 2. Trace V2 Correctness (Schema & Semantics)
- [ ] **Episode Lifecycle**: Always emit `episode_start` and `episode_end` (even on exceptions).
- [ ] **Linking**: `test_id` matches `episode_id` if not explicitly provided.
- [ ] **Prompt Source of Truth**: Config prompt matches Trace prompt exactly (no truncation).
- [ ] **Event Ordering**: Deterministic order (same input -> same JSONL event sequence).
- [ ] **Timestamps**: Consistent `u64` (ms since epoch) or normalized.

## 3. Replay-Strict Invariants (Hard Requirements)
- [ ] **Tool Call ID Consistency (Prio 0)**:
    - `assistant.tool_calls[i].id` == `tool.tool_call_id`.
    - Fallback generation is deterministic (`f"{step_id}:{i}"`).
- [ ] **Tool Args**:
    - JSON parse success -> Object.
    - JSON parse fail -> `{"_raw": "<string>"}` (no crash).
- [ ] **Tool Result Content**:
    - Always a string in tool message content (`json.dumps` with `ensure_ascii=False`).
    - Result in trace is JSON-serializable (via `_jsonable`), never leaks Python objects.
- [ ] **No Silent Drops**:
    - If OpenAI response has no choices -> raise explicit `RuntimeError`.

## 4. Determinism & Idempotency
- [ ] **Determinism**: Same mock input -> Byte-stable trace (IDs, sort order).
- [ ] **File Behavior**: Tests dealing with "re-runs" use truncation or unique IDs to avoid prompt collisions (unless strictly testing ID resolution).
- [ ] **No Magic**: Missing executor -> Explicit `NO_EXECUTOR` error (no silent pass).

## 5. DX & Observability
- [ ] **Error Messages**: Actionable, not cryptical.
- [ ] **Meta Conventions**: Consistent keys (`gen_ai.usage`, `gen_ai.request.model`).
- [ ] **Examples**: `examples/openai-demo` is copy-paste runnable.

## Mandatory Test Matrix (Python SDK)

| Scenario | Script | Expectation |
| :--- | :--- | :--- |
| **A. Smoke (Phase 1.1)** | `tests/e2e/openai_sdk_smoke.sh` | Record -> `verdict ci --replay-strict --db :memory:` -> Exit 0 |
| **B. Tool Loop (Phase 1.2)** | `tests/e2e/openai_tool_loop_smoke.sh` | Tool Call + Result in trace, Replay-Strict OK |
| **C. Workflow Idempotency** | `tests/e2e/openai_tool_loop_idempotency.sh` | Record+CI -> Truncate -> Record+CI -> Exit 0 (Both runs) |
| **D. Negative (Strict Miss)** | *(Manual/Unit)* | Prompt mismatch -> `E_TRACE_MISS` / Exit != 0 |

## CI Commands
```bash
# Build Verdict
cargo build --bin verdict --release --quiet

# Python Path
export PYTHONPATH=$PWD/verdict-sdk/python

# Gates
bash tests/e2e/openai_sdk_smoke.sh
bash tests/e2e/openai_tool_loop_smoke.sh
bash tests/e2e/openai_tool_loop_idempotency.sh
```
