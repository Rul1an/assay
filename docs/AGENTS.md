# Testing Agents with Verdict

Verdict provides first-class support for testing AI Agents, including function calling, tool use sequences, and multi-step reasoning.

## Overview

Testing agents is harder than testing simple RAG pipelines because:
1.  **Non-determinism**: Agents may take different paths (tool calls) to reach the same result.
2.  **Side-effects**: Running agents live (with tools) in CI is slow, expensive, and risky.
3.  **Complexity**: You need to assert on the *intermediate steps* (did it call the search tool?) not just the final answer.

Verdict solves this with:
*   **OpenTelemetry Ingestion**: Record traces from your actual agent framework (LangChain, AutoGen, custom).
*   **Dual-Mode Replay**: Use recorded traces to "replay" the agent's execution without live LLM calls, while verifying assertions against the structured execution graph (Episodes, Steps, Tool Calls).
*   **Behavioral Assertions**: Built-in assertions for tool usage, sequence enforcement, and more.

## Real-World Use Cases (2025)

Verdict is designed for the challenges of modern AI engineering:

### 1. "Compliance-First" Agents (FinTech/Health)
**Context**: Autonomous agents performing sensitive actions (e.g., "block card", "change limit").
**Problem**: Non-determinism in CI is unacceptable for auditors. You need absolute proof that the agent *never* calls unauthorized tools.
**Solution**: `Deterministic Replay` + `Tool Assertions`.
**Value**: Guarantees strict protocol adherence in CI without live LLM calls. Enables true "unit testing" for autonomous agents.

### 2. High-Velocity RAG Pipelines (Cost-Effective CI)
**Context**: Teams shipping daily updates to prompts and retrieval logic.
**Problem**: Running full regression suites with GST-4o for every commit is too slow and expensive.
**Solution**: `Offline Replay Mode` (`--replay-strict`).
**Value**: Developers can test the full flow locally and in CI with 0% LLM cost and millisecond latency.

### 3. Model Migration & Validation (The "Exit Strategy")
**Context**: Migrating from expensive hosted models to specialized, smaller, or on-premise models.
**Problem**: Verifying that the new model is "good enough" without manual review.
**Solution**: `Baseline Regression Testing` (`verdict ci --baseline`).
**Value**: Use existing traces as a baseline to flag semantic deviations in the new model.

## 1. Instrumentation (OpenTelemetry)

Verdict ingests traces via the OpenTelemetry (OTel) GenAI Semantic Conventions.
Most Python/JS frameworks support OTel export.

Ensure your traces include:
*   `gen_ai.prompt` in the span attributes (for the model call).
*   `gen_ai.tool.name` and `gen_ai.tool.args` for tool calls.
*   `gen_ai.completion` for the final response.

## 2. Ingestion & Replay

To enable fast, deterministic CI, we use a "Dual Output" strategy:
1.  **Ingest to DB**: For deep structural assertions (SQL-backed).
2.  **Emit Trace File**: For replay capability (mocking the LLM).

### Workflow

1.  **Record**: Run your agent (locally or in staging) to generate an `otel_trace.jsonl` file.
2.  **Ingest**: Use `verdict trace ingest-otel` to convert this into Verdict's format.

```bash
# Ingest OTel spans -> SQLite DB (assertions) + Replay File (LLM mock)
verdict trace ingest-otel \
  --input otel_trace.jsonl \
  --db .eval/eval.db \
  --suite my-agent-suite \
  --out-trace otel.v2.jsonl
```

3.  **Run Gate**: Run `verdict ci` using the generated replay file.

```bash
# Run assertions using the captured trace data
verdict ci \
  --config eval.yaml \
  --db .eval/eval.db \
  --trace-file otel.v2.jsonl \
  --replay-strict
```

`--replay-strict`: Ensures NO live LLM calls are made. If a prompt is not found in the trace file, the test fails.

### Deterministic Replay (Precedence Rules)

To handle "noisy" traces where multiple model calls or tools might occur, Verdict V0.4.0+ uses strict precedence rules to determine exactly what prompt/output to use for the replay:

**Prompt Extraction**:
1.  **`EpisodeStart`**: If the trace provides an input at start, it wins.
2.  **Model Step**: The first step with `kind="model"` determines the prompt (First Wins).
3.  **Fallback**: If no model step is found, the first step with `gen_ai.prompt` is used.

**Output Extraction**:
1.  **`EpisodeEnd` (Root Span)**: If the Root Span contains `gen_ai.completion`, this takes absolute precedence. This allows the Agent's "Final Answer" to override intermediate tool outputs.
2.  **Last Step**: Otherwise, the last step's completion is used (Last Wins).

## 3. Defining Assertions

Use `eval.yaml` to define behavioral gates for your agent.

### Example Configuration

```yaml
version: 1
suite: my-agent-suite
model: gpt-4
policies:
  agent_policy:
    assertions:
      # 1. Must use a specific tool
      - type: trace_must_call_tool
        tool_name: web_search
        min_calls: 1

      # 2. Must NOT use a forbidden tool
      - type: trace_must_call_tool
        tool_name: delete_database
        max_calls: 0

      # 3. Enforce a specific sequence of actions
      - type: trace_tool_sequence
        sequence:
          - web_search
          - summarize_results
        mode: loose # allow other steps in between
```

### Supported Assertions

*   `trace_must_call_tool`: Verify tool usage counts (min/max).
*   `trace_tool_sequence`: Verify order of operations (`exact` or `loose` modes).
*   `trace_no_tool_errors`: Ensure no tool calls resulted in errors.
*   `trace_max_steps`: Limit the number of steps (prevent infinite loops).

## 4. CI Integration

Check `examples/agent-function-calling/` for a complete, runnable example including:
*   `run.sh`: End-to-end script.
*   `eval.yaml`: complete configuration.
*   `otel_trace.jsonl`: Sample OTel data.
