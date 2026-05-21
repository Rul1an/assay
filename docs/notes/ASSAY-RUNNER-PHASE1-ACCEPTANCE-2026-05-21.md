# Assay-Runner Phase 1 Acceptance

> **Status:** accepted on delegated Linux hardware; internal evidence note,
> not a public release announcement
> **Date:** 2026-05-21
> **Scope:** records the Phase 1 outcome for the internal Assay-Runner
> measured-run spike. It does not imply a public repository split, hosted
> service, macOS support, live LLM support, or a released product surface.

Phase 1 asked whether Assay can produce verifiable measured-run bundles with
low-ambiguity correlation across kernel, policy, and SDK observation layers.

The answer for the delegated Linux/eBPF path is yes.

## Acceptance Run

| Field | Value |
|---|---|
| Workflow | `Runner Spike Delegated` |
| Run | `26211485614` |
| URL | <https://github.com/Rul1an/assay/actions/runs/26211485614> |
| Commit | `56571045` |
| Branch | `codex/assay-runner-drop-kernel-stream-before-stats` |
| Mode | `gates=all` |
| Delegated job duration | `13m11s` |
| Host class | `assay-bpf-runner` self-hosted Linux runner |

The delegated run executed all Phase 1 proof modes in one workflow dispatch:

| Gate | Result |
|---|---|
| `kernel-only` | 3 acceptance runs plus three-run determinism passed |
| `kernel+policy` | 3 acceptance runs plus three-run determinism passed |
| `OpenAI Agents kernel+policy+SDK` | 3 acceptance runs plus three-run determinism passed |

The OpenAI Agents gate exercised the real `@openai/agents` SDK runtime path
with a deterministic local model provider. It did not make live LLM calls.

The determinism claim is over normalized runner evidence artifacts extracted
from the archive. It is not a claim that the raw eBPF object, raw ring-buffer
delivery, or every kernel telemetry event is byte-identical across runs.

## What Was Proven

This run proves the Phase 1 Linux/eBPF delegated path under the bounded spike
contract:

- the eBPF program loads and attaches on the delegated Linux host
- child execution can be placed into a clean measured cgroup before spawn
- kernel observation can complete with `ringbuf_drops=0`
- policy decisions can be correlated with kernel-observed side effects
- SDK tool-call events from the real OpenAI Agents runtime can be correlated
  with policy events by `tool_call_id`
- the spike archive can be verified by the existing Assay evidence path
- each proof mode can produce byte-stable three-run determinism on the
  delegated host

Allowed Phase 1 claim:

```text
For the deterministic acceptance fixtures, Assay can produce verifiable
measured-run bundles on a delegated Linux/eBPF host that correlate kernel,
policy, and OpenAI Agents SDK evidence with low ambiguity and complete
observation health.
```

Forbidden claims remain forbidden:

- this does not prove macOS or Windows kernel-grounded attribution
- this does not prove live LLM execution or cassette replay
- this does not prove arbitrary SDK compatibility beyond the validated
  `@openai/agents` fixture
- this does not prove production traffic, sustained load, or a runner fleet
- this does not prove event-level syscall causality or ordered trace semantics
- this does not include a dedicated ring-buffer drop debug mode; follow-up is
  tracked in <https://github.com/Rul1an/assay/issues/1271>

## Kill Criteria

The Phase 1 spike plan named several ways the track could fail. The delegated
acceptance run mechanically refuted each one for the tested Linux path.

| Kill criterion | Acceptance result |
|---|---|
| Clean cgroup correlation cannot be made reliable | Refuted: all delegated acceptance runs reported clean correlation |
| Ordinary runs produce ring-buffer drops often enough to make complete observation abnormal | Refuted: all delegated acceptance runs in `gates=all` completed with zero drops |
| `tool_call_id` cannot be carried through the OpenAI Agents path | Refuted: OpenAI Agents SDK and policy correlation passed across three runs |
| Bundle verification requires a parallel artifact system | Refuted: the spike bundle path verifies through the existing Assay evidence integration |
| Policy-to-kernel attribution is unstable across repeated runs | Refuted: `kernel+policy` three-run determinism passed |

## Regression Anchors

The green delegated run depended on several concrete discoveries. These are
now regression risks and should remain anchored by code, tests, docs, or
delegated gates before refactors touch the runner path.

| Discovery | Why it matters | Current anchor |
|---|---|---|
| `EVENT_INODE_RESOLVED` is telemetry, not attribution evidence | Including it made bundle output noisy without strengthening the claim | Runner-spike kernel normalizer filter and delegated determinism gate |
| Loader, locale, runtime, and dependency paths are telemetry, not bundle evidence | Dynamic linker and locale opens dominated kernel volume and were not agent behavior | eBPF/userspace path filters and delegated determinism gate |
| Ring-buffer consumers must persist across polls | Rebuilding the consumer per poll replayed kernel records and was the final root cause blocking byte-stable delegated determinism | `assay-monitor` listener ownership of persistent ring buffers |
| Runner session cgroups must start from a valid domain root | Service cgroups can be `domain threaded`; child session cgroups below them can reject process placement | cgroup domain-root resolution and unit coverage |
| Self-hosted workflow cleanup must precede action download | Deleting `_actions` inside the same job can delete already prepared action repositories | delegated workflow prepare job plus gates job split |
| SDK metadata must come from installed package metadata | Hardcoded SDK version strings can silently falsify bundle claims after dependency bumps | OpenAI Agents fixture metadata load plus expected-version gate |
| Acceptance fixtures must be deterministic below output level | Cold/warm fixture differences can change observed syscall surfaces even when files match | fixture pre-seeding/control paths and delegated three-run determinism |

## Phase 2 Follow-Up

Phase 2 should consolidate this proof before any repository split or platform
expansion:

1. freeze versioned v0 references for `observation-health`,
   `capability-surface`, and `correlation-report`:
   [`Runner artifact v0 contracts`](../reference/runner/artifacts-v0.md)
2. maintain the telemetry-versus-evidence filter contract in that reference
3. maintain the delegated runner runbook for provisioning and failure triage:
   [`ASSAY-RUNNER-DELEGATED-RUNBOOK-2026-05-21.md`](../ops/ASSAY-RUNNER-DELEGATED-RUNBOOK-2026-05-21.md)
4. classify when delegated CI is required for runner-impacting changes:
   [`Runner CI lane contract`](../reference/runner/ci-lanes.md)
5. write and maintain the acceptance fixture contract for future SDK fixtures:
   [`Runner acceptance fixture v0 contract`](../reference/runner/fixtures-v0.md)
6. define the Assay-Runner boundary and extraction map

OpenTelemetry mappings, macOS support, live LLM calls, and repository
extraction are outside Phase 2A. They should only start after the Linux runner
boundary is documented and stable.

## Proof Pack Gap

This acceptance note records the workflow run, commit, and pass/fail evidence.
It does not yet retain a durable standalone proof pack containing bundle
digests and selected JSON artifacts. Phase 2 should add a small delegated proof
pack artifact so future acceptance reviews do not depend only on workflow logs.
