# Refactor Wave Status

## Intent
This document is the operational status page for the bounded refactor wave program.

It answers four questions:

1. which waves are closed-loop on `main`
2. which PRs are canonical
3. which hotspot classes were reduced
4. which refactor rules are now standing policy

## Closed-loop waves on `main`

| Wave | Area | Canonical PR line | Status | Result |
|---|---|---|---|---|
| Wave8 | Adapters | adapter split line landed on `main` | Closed-loop | A2A and UCP moved to facade + bounded implementation modules |
| Wave10 | Mandate store | `#621`, `#625`, `#623` | Closed-loop | mandate store internals split into smaller bounded units |
| Wave11 | Registry client tests | `#628`, `#629` | Closed-loop | `registry_client.rs` split into scenario modules + shared support |
| Wave12 | Agentic | `#632`, `#633`, `#634`, `#635` | Closed-loop | `agentic/mod.rs` split into facade, builder, policy helpers, tests |
| Wave13 | Model | Step1 + Step2 landed, `#643`, `#644` | Closed-loop | model monolith moved to bounded module layout |
| Wave14 | ACP adapter | `#646`, `#648`, `#649`, `#652` | Closed-loop | ACP split into facade, convert, mapping, lossiness, normalize, raw payload, tests |
| Wave15 | MCP policy | `#654`, `#656`, `#658`, `#659` | Closed-loop | MCP policy split into facade, engine, legacy, schema, response |
| Wave16 | MCP tool call handler | `#661`, `#663` | Closed-loop | tool call handler split into facade, evaluate, emit, types, tests |
| Wave17 | Replay bundle | `#666`, `#668` | Closed-loop | replay bundle split into bounded replay/bundle modules |
| Wave18 | Mandate types | `#670`, `#672`, `#674`, `#675` | Closed-loop | mandate types split into facade, core, serde, schema, tests |
| Wave19 | Coverage command | Step1 landed, `#679`, `#680` | Closed-loop | coverage command split into facade, generate, legacy, IO, supporting modules |

## What changed
The refactor program reduced large single-file hotspots in these areas:

- adapters
- runtime/mandate internals
- agentic suggestion flow
- MCP governance path
- replay/bundle path
- evidence type surface
- coverage CLI command surface

## Standing refactor policy
The following rules are now default policy:

- use wave slicing:
  - Step1 freeze
  - Step2 mechanical split
  - Step3 closure
  - single clean promote when needed
- closure slices are docs+gate only
- reviewer gates are allowlist-only
- workflow edits are banned unless explicitly scoped
- prefer merge-based sync over rebase for promote hygiene
- do not mix semantic cleanup with mechanical split work
- require post-merge validation on `main`

## Canonical PR rule
When a wave has superseded or obsolete PRs, the canonical PR line is the one that actually lands the final clean scope on `main`.

Superseded PRs must not be treated as source-of-truth.

## Current hotspot posture
Historical hotspot lists are no longer authoritative.

Future wave selection must start from the live `main` tree.

Selection priority:
1. runtime/core governance surfaces
2. evidence/replay surfaces
3. CLI surfaces
4. tests only when they are still dominant hotspots

## How to choose the next wave
Start a new wave only when all of the following are true:

- the target is still large on current `main`
- the target has a bounded public surface
- there is a clear zero-behavior-change split plan
- deterministic contract tests can be pinned in Step1

## Definition of done
A wave is only done when all of the following are true:

- freeze, mechanical, and closure slices are merged
- promote path is merged when applicable
- post-merge checks on `main` are green
- superseded PRs do not remain ambiguous
- no scope leaks remain open

## Notes
This document stays intentionally short.

Detailed wave context remains in each wave's:
- split plan
- checklist
- review pack
- reviewer gate
