# privileged-action PR-gate (runnable, offline)

An agent reaches a GitHub MCP server through the enforcing proxy and tries `github.add_deploy_key` on
`acme/prod-app` — a privileged in-application write. The proxy decides **per call, before it
forwards**, and writes a replayable `assay.enforcement_decision.v0` record. Everything here runs
against a local mock: no real credentials, no real GitHub call.

```bash
./run.sh
```

```text
Privileged action under review: github.add_deploy_key on acme/prod-app

❌ DENY   github.add_deploy_key  reason=no_declared_allowance
❌ DENY   github.add_deploy_key  reason=credential_scope_insufficient
❌ DENY   github.add_deploy_key  reason=manifest_drifted_since_approval
✅ ALLOW  github.add_deploy_key  reason=allow
✅ ALLOW  github.add_deploy_key  reason=allow  + conformance: mismatched (declared_read_only_observed_mutating)  [separate, non-gating]
```

## What each line shows

| Scenario | Policy / baseline / observed surface | Outcome |
|---|---|---|
| not declared | no allowance declares the action | `no_declared_allowance` |
| credential too narrow | credential lacks the required scope | `credential_scope_insufficient` |
| changed since approval | the observed tool surface no longer matches the approved baseline | `manifest_drifted_since_approval` |
| fully declared | declared, scoped, and matching the approved baseline | `allow` (forwarded to the local mock) |
| conformance signal | allowed, but the tool declares `readOnlyHint:true` while the call is mutating | `allow`, plus a **separate** `assay.tool_annotation_conformance.v0` record (`mismatched`) |

The last line is the one to read twice: the conformance signal is recorded **beside** the verdict
and never changes or gates it. A tool that calls itself read-only while doing a mutating action is
worth knowing about; it is evidence, not a denial.

## Bounded non-claims

- a deny is fail-closed caution, not a verdict on intent;
- an allow is the decision to forward, never proof the action happened;
- the example runs against a local mock — no real provider, no real side effect;
- observed behaviour is the proxy's classification of the call, not verification of the upstream
  side effect.

## How it works

`run.sh` drives `assay-mcp-server proxy-enforce` against `mock_github_mcp.py` for each scenario,
varying only the policy (`policies/`), the approved baseline (`baseline-approved*.json`), and the
mock's tool surface (`MOCK_MODE`). It sends one `tools/call`; the proxy's bounded pre-call establish
observes the surface, the PDP decides, and the decision is written to a temporary
`assay.enforcement_decision.v0` record that `run.sh` reads back. No client `tools/list` is sent, so the
observation comes only from the establish step and the run is deterministic.

`./verify.sh` asserts the verdicts and reason codes match `expected-output.txt`.

The example prefers a local build at `../../target/debug/assay-mcp-server`, then an installed
`assay-mcp-server` on `PATH`, otherwise it builds one. Set `PYTHON` to choose the interpreter for the
mock.
