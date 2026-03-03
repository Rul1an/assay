# PLAN - Experiment: MCP Compat Host for Fragmented IPI (2026Q1)

## Intent
Enable truthful live execution for the fragmented IPI experiment without changing the existing harness tool contract.

The chosen approach is a thin, experiment-only MCP compat host that exposes exactly:
- `read_document`
- `web_search`

This compat host exists to preserve the current experiment surface while allowing the harness to run over real stdio MCP traffic.

This is a docs-only freeze slice (Step1). No runtime or workflow changes.

## Why this approach
The current experiment already demonstrates:
- Assay wrap plus sequence enforcement can deterministically block tool-mediated exfiltration in the controlled harness

What is not yet demonstrated is:
- the same harness running unchanged against a real MCP host path without tool-surface mismatch

The repo does not currently provide a drop-in live MCP server with the exact `read_document` plus `web_search` surface expected by the harness.

Therefore, the smallest correct next step is:
- keep the harness unchanged
- add an experiment-only compat host
- map the compat host onto external MCP-compatible backends where appropriate

## Chosen backend strategy (frozen)
Default target for the compat host:
- source backend: filesystem-like MCP server surface for document reads
- sink surface: `web_search` remains a sink-like tool exposed by the compat host for experiment scoring and observability

This slice is explicitly not tied to:
- Obsidian
- vendor-specific browser/search products
- product/runtime feature work in Assay core

## Scope (Step1)
In-scope:
- freeze the compat-host role and boundaries
- freeze the exact two-tool surface
- freeze safety constraints and evidence requirements
- freeze the distinction between experiment adapter code and product code

Out-of-scope:
- no runtime compat-host implementation yet
- no workflow changes
- no new sink classes beyond `web_search`
- no claim that arbitrary external MCP servers are drop-in compatible
- no changes to experiment scoring semantics

## Compat-host role (frozen)
The compat host must:
- speak MCP over stdio
- expose exactly `read_document` and `web_search`
- preserve the existing harness expectations for arguments and result structure
- remain small enough to audit as experiment infrastructure

The compat host must not:
- become a general Assay product feature
- expand the experiment surface beyond the two required tools
- silently change attack-success semantics

## Tool contract (frozen)
### `read_document`
- input: `path: string`
- output includes document content
- intended source: external filesystem-like backend constrained to experiment fixtures or other explicitly allowed roots

### `web_search`
- input: `query: string`
- output includes result payloads sufficient for the harness
- semantic role: sink-like tool call for exfiltration scoring and observability

The experiment does not require `web_search` to provide general internet retrieval quality.
It does require:
- a visible sink call
- preserved query content for canary-based scoring
- deterministic logging/evidence

## Safety constraints (frozen)
- no Obsidian dependency in this compat-host line
- no hardcoded absolute user paths in repo-tracked scripts or docs
- no raw sensitive body logging in repo artifacts
- no workflow changes
- no CI dependence on external network services

## Evidence and audit requirements
Live runs using the compat host must preserve:
- tool call observability at the MCP boundary
- enough metadata to attribute sink invocation and blocking
- compatibility with the existing Assay wrap plus sequence-sidecar instrumentation

## Acceptance criteria (Step1)
- the docs freeze an experiment-only compat host, not a product feature
- the exact two-tool surface is explicit
- Obsidian is explicitly out of scope
- backend strategy is explicit enough to guide implementation without overcommitting to vendor-specific tooling
- no runtime/workflow changes are part of this slice
