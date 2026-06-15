# OWASP MCP Top 10 — Assay Coverage Mapping

How Assay maps to the [OWASP MCP Top 10](https://owasp.org/www-project-mcp-top-10/) security risks.

This mapping is intentionally bounded. Assay can provide strong evidence, gates, and review behavior for
workflows it observes or wraps. It does not replace an enterprise MCP control plane, secret manager,
package registry, or endpoint governance platform.

## Coverage Language

| Level | Meaning |
| --- | --- |
| None | Assay has no relevant control or evidence today. |
| Partial | Assay has detection, evidence, or a narrow control, but not end-to-end governance for the risk. |
| Strong | Assay has deterministic evidence and/or gates for Assay-wrapped or explicitly scanned workflows, with coverage limits reported. |
| Complete | The risk is fully controlled inside the stated scope. Use sparingly; most MCP risks are broader than Assay's observation boundary. |

## Current Mapping

| OWASP Risk | ID | Assay Coverage | How |
| --- | --- | --- | --- |
| Token Mismanagement & Secret Exposure | MCP01 | Partial | Evidence lint and rendered-output hardening reduce leak risk in known artifacts. Strong coverage requires adversarial redaction gates across all public sinks, credential-alias inventory, and explicit no-transport-token-passthrough conformance. Assay should not claim secret lifecycle management, vaulting, or rotation. |
| Privilege Escalation via Scope Creep | MCP02 | Strong | Runtime policy gates, declared credential scopes, least-privilege review, and privileged-action decision records cover Assay-wrapped tool calls. |
| Tool Poisoning | MCP03 | Strong | Tool identity, metadata hashing, manifest drift detection, and signed/declared tool surfaces cover observed tool-surface changes. Behavior drift under identical metadata remains out of scope unless separately observed. |
| Supply Chain Attacks & Dependency Tampering | MCP04 | Partial | Assay verifies some package, pack, adapter, and manifest digests. Strong coverage requires MCP server admission records, runtime-vs-admission digest comparison, and registry/package/source drift fixtures. Assay should not claim registry ecosystem security. |
| Command Injection & Execution | MCP05 | Strong | Policy deny/restrict rules, argument constraints, and sandbox/degradation evidence cover Assay-run commands and wrapped tool invocations. |
| Intent Flow Subversion | MCP06 | Strong | Sequence policy checks, delegation/context projections, and review artifacts cover explicit, observed flow violations. Prompt-context attacks outside supplied artifacts remain out of scope. |
| Insufficient Authentication & Authorization | MCP07 | Strong | Approval, policy, caller, credential-alias, and scope gates cover Assay-wrapped decisions. External provider authorization remains the provider's authority unless imported as evidence. |
| Lack of Audit and Telemetry | MCP08 | Strong | Evidence bundles, decision logs, replay, diff, lint, SARIF, and Plimsoll review provide strong auditability for Assay-wrapped runs. Not complete for unwrapped/shadow MCP servers or external provider audit unless imported. |
| Shadow MCP Servers | MCP09 | Partial | Discovery can list some local MCP servers and the enforcing proxy protects wrapped servers. Strong coverage requires inventory artifacts, declared server allowlists, coverage-honest scanning, and review findings for unapproved or drifted servers. Assay should not claim absence outside scanned sources. |
| Context Injection & Over-Sharing | MCP10 | Strong | Redaction, argument filtering, context-boundary checks, and receipt-boundary guards cover supplied evidence and wrapped calls. Assay does not govern model memory outside observed artifacts. |

## Priority Path To Stronger Coverage

The next evidence wave should not immediately relabel MCP01, MCP04, or MCP09 as strong. It should first
prove the controls with bounded experiments:

1. **MCP09 Shadow MCP foundation** — inventory configured MCP servers, report scan coverage, and flag observed unapproved or drifted servers without claiming absence outside scanned sources.
2. **MCP01 Secret/credential-boundary gate** — run an adversarial leak corpus across public sinks and credential evidence, proving raw secrets and control characters do not render.
3. **MCP04 MCP server admission/provenance** — compare declared server admission records with observed runtime server and manifest evidence, reporting drift or unknown provenance without maliciousness claims.

## Non-Claims

- Strong coverage is scoped to Assay-wrapped or explicitly scanned workflows.
- Not observed is not absent unless coverage is complete for the relevant sources.
- Admitted is not safe; it only means the server/source matched a declared admission record.
- Digest match is not benign behavior; digest drift is not maliciousness.
- Unsigned or unknown provenance is not maliciousness; it is a review/coverage condition.
- Secret redaction in public sinks does not prove secrets never existed in the source system.

## Sources

- [OWASP MCP Top 10](https://owasp.org/www-project-mcp-top-10/)
- [OWASP MCP01 Token Mismanagement & Secret Exposure](https://owasp.org/www-project-mcp-top-10/2025/MCP01-2025-Token-Mismanagement-and-Secret-Exposure)
- [OWASP MCP09 Shadow MCP Servers](https://owasp.org/www-project-mcp-top-10/2025/MCP09-2025%E2%80%93Shadow-MCP-Servers)
- [Microsoft OWASP MCP Top 10 Security Guidance for Azure](https://microsoft.github.io/mcp-azure-security-guide/)
