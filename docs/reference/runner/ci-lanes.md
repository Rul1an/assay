# Assay-Runner CI Lane Contract

> Internal Phase 2A reference. This page classifies when ordinary continuous
> CI is enough and when the delegated Linux/eBPF runner lane is required.

Assay-Runner has two proof classes:

- **Continuous CI**: hosted or ordinary repository checks that must stay fast,
  broadly available, and safe for pull requests.
- **Delegated CI**: the manual `Runner Spike Delegated` workflow on
  `[self-hosted, linux, assay-bpf-runner]`, used when a change can only be
  proven on the dedicated Linux/eBPF host.

The delegated lane is intentionally `workflow_dispatch` only. Do not add a
pull-request, push, or schedule trigger to make the lane automatic. The host is
dedicated and destructive cleanup is part of the contract.

## Decision Table

| Change type | Continuous CI | Delegated CI | Required before merge? |
|---|---|---|---|
| Docs-only change outside runner acceptance, runbook, artifact contracts, or CI lane docs | yes | no | no |
| Docs-only change to runner acceptance, runbook, artifact contracts, fixture contract, or CI lane docs | yes | no | no, unless it changes acceptance criteria |
| Non-runner crate change with no monitor, policy, SDK fixture, cgroup, workflow, or evidence artifact impact | yes | no | no |
| Runner artifact schema, field set, note format, or artifact determinism semantics | yes | yes | yes |
| Runner telemetry-versus-evidence filter behavior | yes | yes | yes |
| `assay-monitor` ring-buffer reader, BPF event decode, drop accounting, or capture stats | yes | yes | yes |
| `crates/assay-monitor/**` or `crates/assay-ebpf/**` | yes | yes | yes |
| eBPF build image, build command, or loader/attach path | yes | yes | yes |
| Runner cgroup placement, domain-root resolution, or process spawn discipline | yes | yes | yes |
| Policy correlation, policy event normalization, or policy-to-kernel coherence rule | yes | yes | yes |
| OpenAI Agents fixture dependency, SDK event normalization, SDK version assertion, or `tool_call_id` binding | yes | yes | yes |
| Acceptance fixture behavior or control paths | yes | yes | case-by-case; required when observed evidence can change |
| Delegated workflow, runner labels, cleanup, checkout, sudo environment, preflight, or `scripts/ci/runner-spike-*.sh` | yes | yes | yes |
| `@openai/agents`, `zod`, or fixture `package-lock.json` bump | yes | yes | yes |
| `aya`, `aya-ebpf`, `aya-log-ebpf`, or BPF/runtime dependency bump | yes | yes | yes |
| Workspace dependency bump that can affect `assay-runner-spike`, `assay-monitor`, `assay-ebpf`, `assay-cli`, policy correlation, or runner fixtures | yes | yes | yes |
| Delegated runbook wording that only clarifies existing behavior | yes | no | no |
| Follow-up issue text, planning notes, or extraction-roadmap prose | yes | no | no |

## Required Delegated Gate

When delegated CI is required, choose the narrowest gate that proves the
changed layer:

| Touched surface | Minimum delegated gate |
|---|---|
| Kernel fixture control path or kernel-only acceptance assertion | `kernel-only` |
| Policy capture or policy-to-kernel coherence | `kernel-policy` |
| OpenAI Agents SDK fixture, SDK event schema, SDK version assertion, or `tool_call_id` binding | `openai-agents-kernel-policy` |
| `crates/assay-monitor/**`, `crates/assay-ebpf/**`, eBPF build/attach path, cgroup placement, telemetry filter, cross-layer archive, artifact schema, correlation report, workflow/security model, runner scripts, BPF/runtime dependency bump, or final release/acceptance proof | `all` |

If a change touches multiple surfaces, run the highest required gate. If the
right gate is ambiguous, default to `all`.

Use `build_ebpf=true` for delegated proof unless the workflow has been
explicitly extended to restore a deterministic prebuilt eBPF object after
cleanup and checkout.

## Merge Discipline

1. Run ordinary continuous CI for every PR.
2. For runner-impacting changes, dispatch the minimum delegated gate after the
   PR branch contains the final candidate commit.
3. Record the workflow run URL, commit SHA, selected gate, and result in the
   PR body or a PR comment.
4. Do not treat a delegated skip as success. In the delegated lane, exit `40`
   means the runner contract has drifted.
5. Do not bypass required repository checks for runner-impacting changes.

If a runner-impacting PR cannot get a delegated run because the host is
unavailable, leave the PR open. Merge only docs-only or clearly
non-runner-impacting changes without delegated proof.

The `Assay-Runner Lane Check / lane-check` workflow provides the
machine-visible path classification and delegated proof check for this
contract. To make the contract hard-blocking, repository branch protection must
mark that check as required.

The executable path mapping lives in
`scripts/ci/assay_runner_lane_check.py` and must mirror the decision table on
this page. Changes to this contract and changes to the classifier must land in
the same PR; the helper's `--self-test` is the drift canary for the known
runner-impacting surfaces.

Ring-buffer drop diagnostics remain a separate follow-up tracked in
<https://github.com/Rul1an/assay/issues/1271>; that issue must not weaken the
`ringbuf_drops=0` delegated acceptance rule.

## Boundary Rule

The delegated lane proves the Linux/eBPF runner boundary, not macOS, Windows,
live LLM calls, production load, or a distributed runner fleet. Platform
expansion requires a separate platform spike with its own kill criteria and CI
lane contract.
