# Slice 7 Delegated Baseline Smoke

> **Status:** delegated-baseline-smoke-verified.
> **Scenario:** `matched_safe_read`.
> **Run:** [`26571739019`](https://github.com/Rul1an/assay/actions/runs/26571739019).
> **Proof pack:** `assay-runner-delegated-proof-pack-26571739019`
> (artifact `7264883391`, retained until 2026-08-26).
> **Assay commit:** `c3384c425673fe09b0368f82765c72dda86ac200`.

## Outcome

The delegated `openai-agents-kernel-policy` gate passed all three
deterministic OpenAI Agents kernel+policy runs. The proof pack records
three Runner archive tarballs, selected JSON, gate logs, and four pass
lines: one acceptance pass for each run plus the three-run determinism
pass.

The delegated baseline is therefore a `positive_join`, not a
semantic-gap finding. It proves the positive join path for the existing
OpenAI Agents fixture under real Runner capture.

## Evidence

| Check | Result |
|---|---|
| Gate | `openai-agents-kernel-policy` |
| Workflow inputs | `gates=openai-agents-kernel-policy`, `build_ebpf=true` |
| Runner health | `kernel_layer=complete`, `ringbuf_drops=0`, `cgroup_correlation=clean` |
| SDK evidence | one `tool_call_started` and one `tool_call_completed` for `tc_runner_policy_001`, tool `read_file` |
| Policy evidence | `allow` decision for `tc_runner_policy_001`, tool `read_file` |
| Kernel evidence | two successful workdir-bounded `openat` read events |
| Correlation | clean, one binding for `tc_runner_policy_001`, zero ambiguities |
| Join result | strong `tool_call_id` join, no fallback |
| Scenario verdict | `positive_join` |

## Implementation Note

The first delegated attempts exposed a cgroup-root issue under the
self-hosted `assay-bpf-runner` systemd service: using a `.service` unit
as the session root can become invalid for child process placement once
systemd reports the service cgroup as threaded. The fix in this branch
treats `.service` units like `.scope` units and ascends to the nearest
non-leaf domain cgroup before creating Assay session cgroups.

## Non-Claims

- This smoke does not publish delegated semantic-gap findings.
- This smoke does not dispatch delegated gap scenarios.
- This smoke does not promote evidence packs, semantic-gap verdicts, or
  join rows to product APIs.
- This smoke does not rank Runner, OTel, OpenInference, or the OpenAI
  Agents SDK.
