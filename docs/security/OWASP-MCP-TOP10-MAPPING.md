# OWASP MCP Top 10 — Assay Coverage Mapping

How Assay addresses the [OWASP MCP Top 10](https://owasp.org/www-project-mcp-top-10/) security risks.

| OWASP Risk | ID | Assay Coverage | How |
|-----------|-----|---------------|-----|
| Token Mismanagement & Secret Exposure | MCP01 | Strong (scoped) | Render-safety pipeline (control-strip → redact → bound → sink-encode) over the Assay CLI rendered sinks (console, `run.json`, SARIF, JUnit) and the Plimsoll review sinks, conformance-verified; redaction precedes bounding so a secret cannot survive as a truncated prefix. Proxy credential-boundary conformance (`assay.token_passthrough_conformance.v0`) shows a consumed inbound auth value is not re-emitted on outbound headers/body/env. Evidence lint also flags secrets in subjects (`ASSAY-W001`); policy can deny credential-exposing tools. Pattern-based detection, scoped to rendered sinks. Plimsoll consumer private. See note below. |
| Privilege Escalation via Scope Creep | MCP02 | Strong | `restrict_scope` enforcement limits tool arguments at runtime. Policy constraints enforce path/param boundaries. |
| Tool Poisoning | MCP03 | Strong | Tool signing (`x-assay-sig`), identity verification, tool metadata hashing. [Delegation spoofing experiment](../architecture/RESULTS-EXPERIMENT-DELEGATION-SPOOFING-2026q2.md) tested trust-domain verification. |
| Supply Chain Attacks & Dependency Tampering | MCP04 | Partial | Pack digest verification (SHA-256/JCS). Adapter identity pinning. Lockfile support in registry client. |
| Command Injection & Execution | MCP05 | Strong | Policy `deny` rules block `exec`/`shell`/`bash`. Argument validation via regex constraints. Landlock sandbox for runtime isolation. |
| Intent Flow Subversion | MCP06 | Strong | Sequence policies detect tool-call ordering violations. [Memory poisoning experiment](../architecture/RESULTS-EXPERIMENT-MEMORY-POISON-2026q2.md) tested delayed payload reactivation. |
| Insufficient Authentication & Authorization | MCP07 | Strong | `approval_required` enforcement, mandate system with revocation, auth context validation. |
| **Lack of Audit and Telemetry** | **MCP08** | **Complete** | **Evidence bundles, decision logs, replay, diff, lint, SARIF output.** This is Assay's primary value proposition. |
| Shadow MCP Servers | MCP09 | Strong (scoped) | Assay inventory carrier (`assay.mcp_server_inventory.v0`, coverage-honest, command/args hashed) + Plimsoll allowlist review over scanned config/process sources; coverage limits reported. No absence claim outside scanned sources. Plimsoll consumer private. See note below. |
| Context Injection & Over-Sharing | MCP10 | Strong | `redact_args` enforcement strips sensitive fields. Context envelope hardening validates completeness. [Protocol evidence experiment](../architecture/RESULTS-EXPERIMENT-PROTOCOL-EVIDENCE-INTERPRETATION-2026q2.md) tested consumer-side interpretation. |

> **Note on MCP01 "Strong (scoped)".** "Strong" here means rendered outputs carry no raw credential / PII / terminal-control values. A render-safety pipeline redacts before bounding (so a secret cannot survive as a truncated prefix) across the Assay CLI rendered sinks (console, `run.json`, SARIF, JUnit) and the Plimsoll review sinks, proven by render-safety conformance over a hostile corpus; the proxy does not re-emit a consumed inbound auth value (token-passthrough conformance). Detection is pattern-based and may miss a novel secret format. Scope is rendered sinks — not capture-side redaction, and not token lifecycle / rotation / vaulting. The Assay CLI render-safety is public; the Plimsoll review consumer is currently private. It is not Complete coverage.

> **Note on MCP09 "Strong (scoped)".** "Strong" here means the workflow has deterministic producer evidence and a validated review consumer. The public Assay artifact is the inventory carrier (`assay mcp inventory`); the Plimsoll review consumer is currently private, so public users can reproduce the evidence producer but not the full review/gating path from this repository alone. It does not mean Assay detects all shadow MCP servers, and it is not Complete coverage.

## Summary

Assay provides **Strong** or **Complete** coverage for 9 of 10 OWASP MCP risks, with **Partial** coverage for the remaining 1 (MCP04). MCP01 and MCP09 are **Strong (scoped)** — see the notes above.

The strongest alignment is with **MCP08 (Lack of Audit and Telemetry)** — Assay's evidence bundles, decision logs, and replay capabilities are a direct and comprehensive answer to this risk.

## Security Experiments

Assay's coverage claims are backed by [three bounded security experiments](../architecture/SYNTHESIS-TRUST-CHAIN-TRIFECTA-2026q2.md) testing 12 attack vectors across producer, adapter, and consumer perspectives. All experiments achieved zero false positives under the full contract stack.
