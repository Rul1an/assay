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

## Priority Experiment Wave: MCP01 / MCP04 / MCP09

The current Partial rows should move only after bounded experiments prove observation, classification,
claim honesty, and fail-closed behavior. The priority is MCP09 first because shadow servers define the
coverage boundary for MCP01, MCP04, and MCP08.

| Priority | Experiment | Primary risks | First artifact or report | Claim |
| --- | --- | --- | --- | --- |
| 1 | M1 — MCP inventory coverage / shadow-server zoo | MCP09 | `assay.mcp_server_inventory.v0` | Configured MCP servers can be inventoried from declared sources; unapproved or drifted observed servers are reportable. |
| 2 | M3 — Secret sink and credential-boundary corpus | MCP01 | `assay.experiment.mcp01_secret_sink.v0` | Public sinks and credential evidence do not render raw secrets from the adversarial corpus. |
| 3 | M2 — Declared-vs-observed MCP server admission | MCP09, MCP04 | `assay.mcp_server_admission.v0` | Observed runtime servers can be compared with declared admission records without claiming safety or maliciousness. |
| 4 | M4 — MCP supply-chain drift zoo | MCP04 | supply-chain drift fixture corpus | Source/package/manifest drift is classified as review evidence, not maliciousness. |
| 5 | M5 — Cross-risk chain: shadow server → secret exposure → missing audit | MCP09, MCP01, MCP08 | multi-risk review fixture | Multiple bounded findings can be reported without collapsing into one overbroad verdict. |
| 6 | M6 — OWASP MCP coverage report projection | MCP01, MCP04, MCP09, MCP08 | `plimsoll.owasp_mcp_coverage_report.v0` | Coverage is projected from source artifact digests; incomplete sources never read as complete. |

### Shared Acceptance Rules

- Not observed is not absent unless scanner coverage is complete for the relevant source class.
- Observed is not approved; admitted is not safe.
- Digest drift and unsigned source are review conditions, not maliciousness findings.
- Coverage incomplete means warning/inconclusive/pending, never clean.
- Every projected coverage statement links back to source artifact digests.
- Redaction happens before projection/truncation for public sinks.

### First Build Target

Start with **M1 — MCP inventory coverage / shadow-server zoo**.

Minimum acceptance:

- approved server unchanged → no finding;
- unapproved configured server → `shadow_mcp_server_observed`;
- approved id with changed command or args digest → drift finding;
- duplicate server id with different command → identity finding;
- HTTP MCP endpoint outside allowlist → unapproved endpoint finding;
- no servers found with incomplete scanner coverage → inconclusive/warning, not clean.
