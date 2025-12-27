# MCP Integration

Assay is built for the [Model Context Protocol](https://modelcontextprotocol.io).

---

## What is MCP?

**Model Context Protocol (MCP)** is an open standard for connecting AI agents to external tools and data sources. It defines how agents:

- Discover available tools (`tools/list`)
- Call tools with arguments (`tools/call`)
- Receive results

Assay validates these interactions to ensure your agent behaves correctly.

---

## Assay's Role in the MCP Stack

```
┌─────────────────────────────────────────────────────────────┐
│                        Your Agent                           │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                     MCP (Connectivity)                      │
│              "How agents talk to tools"                     │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                   Assay (Quality Engineering)               │
│        "Are those conversations correct, safe, repeatable?" │
└─────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────┐
│                      External Tools                         │
│              Databases, APIs, File Systems                  │
└─────────────────────────────────────────────────────────────┘
```

**MCP without Assay** = unverified traffic.

---

## Integration Patterns

Assay integrates with MCP in three ways:

### 1. Trace Consumer (Offline Testing)

Import MCP sessions and run deterministic tests in CI.

```bash
assay import --format mcp-inspector session.json
assay run --config mcp-eval.yaml --strict
```

**Use case:** CI regression gates, debugging, baseline comparison.

[:octicons-arrow-right-24: Quick Start](quickstart.md)

---

### 2. MCP Server (Runtime Validation)

Expose Assay as MCP tools that agents call before executing actions.

```bash
assay mcp-server --port 3001 --policy policies/
```

The agent can query:
- `assay_check_args` — "Is this argument valid?"
- `assay_check_sequence` — "Is this call order allowed?"
- `assay_policy_decide` — "Should I proceed?"

**Use case:** Agent self-correction, runtime guardrails.

[:octicons-arrow-right-24: Server Guide](server.md)

---

### 3. MCP Gateway (Enterprise)

Inline enforcement for production deployments.

```
Agent ──► Assay Gateway ──► MCP Server ──► Tools
              │
              └─► Capture, Redact, Enforce, Sign
```

**Use case:** Compliance logging, policy enforcement, audit trails.

[:octicons-arrow-right-24: Gateway Guide](gateway.md) *(Enterprise)*

---

## What Assay Validates

MCP standardizes **how** agents communicate. Assay validates **what** they communicate.

| Validation | Question | Metric |
|------------|----------|--------|
| **Argument Correctness** | Are tool arguments schema-valid? | `args_valid` |
| **Sequence Validity** | Are calls in the right order? | `sequence_valid` |
| **Blocklist Enforcement** | Was a forbidden tool called? | `tool_blocklist` |
| **Replay Fidelity** | Can we reproduce this incident? | `replay` |

---

## Supported Formats

### Import Formats

| Format | Source | Command |
|--------|--------|---------|
| MCP Inspector | [MCP Inspector](https://github.com/modelcontextprotocol/inspector) | `--format mcp-inspector` |
| JSON-RPC 2.0 | Raw MCP messages | `--format jsonrpc` |
| LangChain | LangChain traces | `--format langchain` *(coming soon)* |
| LlamaIndex | LlamaIndex traces | `--format llamaindex` *(coming soon)* |

### Export Formats

| Format | Use Case | Flag |
|--------|----------|------|
| SARIF | GitHub Code Scanning | `--output sarif` |
| JUnit | CI test results | `--output junit` |
| JSON | Programmatic access | `--output json` |

---

## Quick Comparison

| Feature | MCP Alone | MCP + Assay |
|---------|-----------|-------------|
| Tool discovery | ✅ | ✅ |
| Tool execution | ✅ | ✅ |
| Argument validation | ❌ | ✅ |
| Sequence enforcement | ❌ | ✅ |
| Blocklist | ❌ | ✅ |
| Deterministic replay | ❌ | ✅ |
| CI integration | ❌ | ✅ |
| Offline testing | ❌ | ✅ |

---

## Next Steps

<div class="grid cards" markdown>

-   :material-rocket-launch:{ .lg .middle } __Quick Start__

    ---

    Import your first MCP session in 5 minutes.

    [:octicons-arrow-right-24: Quick Start](quickstart.md)

-   :material-server:{ .lg .middle } __Assay MCP Server__

    ---

    Let agents validate their own actions.

    [:octicons-arrow-right-24: Server Guide](server.md)

-   :material-robot:{ .lg .middle } __Self-Correction__

    ---

    Build agents that fix their own mistakes.

    [:octicons-arrow-right-24: Self-Correction](self-correction.md)

-   :material-file-import:{ .lg .middle } __Import Formats__

    ---

    Supported log formats and conversion.

    [:octicons-arrow-right-24: Import Formats](import-formats.md)

</div>
