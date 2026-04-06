# Assay Examples

Ready-to-use examples to get started with Assay.

## Start here

### [MCP Quickstart](./mcp-quickstart)
Wrap an MCP server with policy enforcement in under 2 minutes.
See ALLOW/DENY decisions for every tool call.

## Evaluation examples

### [RAG Grounding](./rag-grounding)
Evaluate if your RAG pipeline answers strictly based on context.
**Metrics**: `semantic_similarity`, `must_contain`, `must_not_contain`.

## Interop examples

### [Google ADK Evaluation Evidence](./adk-evaluation-evidence)
Map one tiny Google ADK evaluation artifact into Assay-shaped external evidence.
**Focus**: evaluation/artifact-first seam, trajectory as observed reference only, no imported evaluator truth.

### [AGT Audit Evidence](./agt-audit-evidence)
Map a tiny AGT `mcp-trust-proxy`-style audit corpus into Assay-shaped external evidence.
**Focus**: allow/deny audit decisions, malformed import failure, no imported trust semantics.

### [CrewAI Event Evidence](./crewai-event-evidence)
Export a small CrewAI event-listener artifact and map it into Assay-shaped external evidence.
**Focus**: bounded task/tool events, optional MCP bonus path, no imported trust semantics.

### [LangGraph Task Evidence](./langgraph-task-evidence)
Export a tiny LangGraph `tasks` v2 stream artifact and map it into Assay-shaped external evidence.
**Focus**: OSS-native tasks seam hypothesis, minimal checkpointer dependency, no imported orchestration truth.

### [OpenAI Agents Trace Evidence](./openai-agents-trace-evidence)
Export a tiny OpenAI Agents trace artifact through a local custom `TraceProcessor`.
**Focus**: trace-processor-first seam, bounded local export, no imported runtime truth.

## 2. [Negation Safety](./negation-safety)
Ensure model adheres to critical safety instructions (e.g. "Do NOT").
**Metrics**: `regex`.

## 3. [Baseline Gate (CI)](./baseline-gate)
Full workflow demonstration of **Regression Testing** with Baselines.
**Features**: `--baseline`, `--export-baseline`.

## 4. [Python SDK Demo](./python-sdk-demo)
Native Python integration using `pytest` and `assay` library.
**Features**: `AssayClient`, `Coverage`, `pytest` integration.

## Usage
You can run any example directly from the root:

```bash
assay run --config examples/rag-grounding/eval.yaml --trace-file examples/rag-grounding/traces/good.jsonl
```
