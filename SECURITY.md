# Security Policy

Assay is a security-critical component in the AI Agent stack. We take vulnerability reporting and supply chain security seriously.

## Supported Versions

We provide security updates for the latest major version.

| Version | Status |
| :--- | :--- |
| **v1.x** | ✅ **Supported** |
| v0.x | ❌ EOL |

## Reporting a Vulnerability

**Please do not report security vulnerabilities through public GitHub issues.**

If you believe you have found a security vulnerability in Assay (Core, CLI, or MCP Server), please report it privately:

*   **Email**: `security@assay.dev`
*   **GitHub**: Use the "Report a vulnerability" tab in this repository.

We aim to acknowledge receipt within 24 hours and provide a timeline for triage.

## Threat Model

Assay is designed to run in untrusted environments (CI/CD, Agent Sandbox).

### In Scope
*   **Policy Bypass**: Bypassing configured `deny` lists or regex constraints.
*   **Remote Code Execution (RCE)**: Triggering arbitrary code execution via a malicious configuration file or trace payload.
*   **MCP Protocol Violation**: Exploits that allow unauthorized tool calls to leak through the `assay-mcp-server` proxy.

### Out of Scope
*   **Physical Access**: Attacks requiring physical access to the machine.
*   **Denial of Service (DoS)**: While we aim for resilience, extreme resource exhaustion attacks are currently lower priority than integrity violations.

## Supply Chain Security

*   **Crates.io**: We use Trusted Publishing (OIDC) to link GitHub Actions directly to Crates.io, eliminating long-lived secret tokens.
*   **PyPI**: We use Trusted Publishing for Python wheels.
*   **Dependencies**: We audit dependencies using `cargo-deny` in our CI pipeline.
