# Identity: What is Assay?

Assay is **not** an evaluation framework (like Ragas or DeepEval).
Assay is a **Policy Engine** and **Compliance Gate**.

## The Problem
Agentic systems are non-deterministic. They call tools in unpredictable orders with unpredictable arguments.
Documentation says "Search before Escalate", but your Agent escalates immediately.

## The Solution
**Assay uses Policy-as-Code to enforce limits.**

-   **Strict Schema**: "Argument `query` must be > 5 chars."
-   **Sequence Rules**: "`search_kb` MUST PRECEDE `escalate_ticket`."
-   **Forbidden Tools**: "Never call `delete_user` in prod."

## Who is it for?

### The "Vibecoder" (AI Operator)
You are building agents with natural language. You need a **Guardrail** that screams red when the LLM hallucinates a command that breaks production.

### The Senior Engineer
You need **CI/CD determinism**. You don't want "flaky evals" that pass 80% of the time based on LLM whims. Assay is deterministic: The input either matches the schema, or it fails.

## Key Principles
1.  **Zero Fluff**: Reports are binary (Pass/Fail), not "Looks LGTM".
2.  **Stateless**: Validates anywhere (CLI, CI, Python, Rust).
3.  **Fast**: Rust-core performance (<10ms overhead).
