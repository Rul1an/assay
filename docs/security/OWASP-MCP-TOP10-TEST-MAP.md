# OWASP MCP Top 10 Test Map

This map connects concrete Assay tests to the OWASP MCP Top 10 risk categories. It complements the product-level coverage summary in `docs/security/OWASP-MCP-TOP10-MAPPING.md`.

Source taxonomy: <https://owasp.org/www-project-mcp-top-10/>

## Current Security-Coverage Map

| OWASP MCP risk | Assay area | Representative tests | Current posture |
| --- | --- | --- | --- |
| MCP01 Token Mismanagement & Secret Exposure | MCP tool-call handling, Trust Basis auth projection, MCP proxy logs | `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs::redact_args_contract_sets_additive_fields`; `crates/assay-evidence/src/trust_basis/tests.rs::trust_basis_g3_absent_when_auth_issuer_jws_shaped_or_principal_bearer`; `crates/assay-cli/tests/e2e_mcp_wrap_assert_cmd.rs::owasp_mcp01_token_args_do_not_leak_to_proxy_logs` | Covered for redaction/auth-projection leak prevention and proxy audit/decision logs omitting raw token-like tool args. |
| MCP02 Privilege Escalation via Scope Creep | MCP policy engine and tool-call handler | `crates/assay-core/src/mcp/tool_call_handler/tests/scope.rs`; `crates/assay-core/tests/policy_engine_test.rs` deny/restrict-scope cases | Covered for restrict-scope enforcement and deny precedence. |
| MCP03 Tool Poisoning | MCP proxy/tool identity and decision emission | `cargo test -p assay-core --lib proxy_contract_`; `crates/assay-core/src/mcp/proxy/tools.rs::owasp_mcp03_metadata_poisoning_description_drift_denies_pinned_tool`; `crates/assay-core/src/mcp/tool_call_handler/tests/emission.rs::test_tool_drift_deny_emits_alert_obligation_outcome` | Covered for tool drift/identity decision evidence and proxy-observed metadata poisoning via description drift. |
| MCP05 Command Injection & Execution | Sandbox and MCP policy deny paths | `crates/assay-cli/src/cli/commands/sandbox.rs` conflict/degradation tests; `crates/assay-cli/tests/e2e_mcp_wrap_assert_cmd.rs` deny command fixtures; `crates/assay-cli/tests/profile_integration_test.rs::owasp_mcp05_sandbox_keeps_shell_metacharacters_as_argv` | Covered for deny/degrade behavior and sandbox command construction preserving shell metacharacters as argv instead of executing them. |
| MCP06 Intent Flow Subversion / Prompt Injection via Contextual Payloads | MCP delegation/context contracts and Trust Basis delegation claims | `crates/assay-core/src/mcp/policy/engine.rs` delegation-context tests; `crates/assay-core/src/mcp/tool_call_handler/tests/delegation.rs`; `trust_basis_detects_supported_delegation_and_degradation` | Covered for explicit context/delegation projection; add prompt-context fixture coverage when proxy surfaces new context inputs. |
| MCP08 Lack of Audit and Telemetry | MCP proxy decisions, sandbox degradation evidence, Trust Basis diff | `cargo test -p assay-core --lib mcp::proxy::`; `crates/assay-cli/tests/evidence_test.rs::test_evidence_export_includes_sandbox_degraded_event_when_profile_contains_degradation`; `trust_basis_contract_diff_report_ordering_is_frozen` | Strong coverage; preserve event emission and degradation payload contracts during splits. |
| MCP10 Context Injection & Over-Sharing | Redaction, Trust Basis external receipt guards | `crates/assay-core/src/mcp/tool_call_handler/tests/redaction.rs`; `trust_basis_rejects_decision_receipt_boundary_when_context_or_metadata_leaks_in` | Covered for redaction and external receipt boundary guards. |

## Residual Backlog

- Keep Trust Basis external receipt guards mutation-smoke eligible after classifier refactors.
- Add prompt/context injection fixtures when proxy surfaces new context inputs beyond explicit tool-call args.
