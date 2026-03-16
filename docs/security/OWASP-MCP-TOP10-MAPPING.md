# OWASP MCP Top 10 — Assay Coverage Mapping

How Assay addresses the [OWASP MCP Top 10](https://owasp.org/www-project-mcp-top-10/) security risks.

| OWASP Risk | ID | Assay Coverage | How |
|-----------|-----|---------------|-----|
| Token Mismanagement & Secret Exposure | MCP01 | Partial | Evidence lint detects secrets in subjects (`ASSAY-W001`). Policy can deny tools that expose credentials. |
| Privilege Escalation via Scope Creep | MCP02 | Strong | `restrict_scope` enforcement limits tool arguments at runtime. Policy constraints enforce path/param boundaries. |
| Tool Poisoning | MCP03 | Strong | Tool signing (`x-assay-sig`), identity verification, tool metadata hashing. [Delegation spoofing experiment](../architecture/RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2.md) tested trust-domain verification. |
| Supply Chain Attacks & Dependency Tampering | MCP04 | Partial | Pack digest verification (SHA-256/JCS). Adapter identity pinning. Lockfile support in registry client. |
| Command Injection & Execution | MCP05 | Strong | Policy `deny` rules block `exec`/`shell`/`bash`. Argument validation via regex constraints. Landlock sandbox for runtime isolation. |
| Intent Flow Subversion | MCP06 | Strong | Sequence policies detect tool-call ordering violations. [Memory poisoning experiment](../architecture/RESULTS-EXPERIMENT-MEMORY-POISON-2026q2.md) tested delayed payload reactivation. |
| Insufficient Authentication & Authorization | MCP07 | Strong | `approval_required` enforcement, mandate system with revocation, auth context validation. |
| **Lack of Audit and Telemetry** | **MCP08** | **Complete** | **Evidence bundles, decision logs, replay, diff, lint, SARIF output.** This is Assay's primary value proposition. |
| Shadow MCP Servers | MCP09 | Partial | `assay discover` lists MCP servers on the machine. Policy enforcement only applies to wrapped servers. |
| Context Injection & Over-Sharing | MCP10 | Strong | `redact_args` enforcement strips sensitive fields. Context envelope hardening validates completeness. [Protocol evidence experiment](../architecture/RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md) tested consumer-side interpretation. |

## Summary

Assay provides **Strong** or **Complete** coverage for 7 of 10 OWASP MCP risks, with **Partial** coverage for the remaining 3.

The strongest alignment is with **MCP08 (Lack of Audit and Telemetry)** — Assay's evidence bundles, decision logs, and replay capabilities are a direct and comprehensive answer to this risk.

## Security Experiments

Assay's coverage claims are backed by [three bounded security experiments](../architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) testing 12 attack vectors across producer, adapter, and consumer perspectives. All experiments achieved zero false positives under the full contract stack.
