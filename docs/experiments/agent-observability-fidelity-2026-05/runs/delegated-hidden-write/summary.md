# Delegated Hidden Write Smoke

> **Status:** delegated-gap-smoke-verified.
> **Scenario:** `hidden_write`.
> **Run:** [`26620643517`](https://github.com/Rul1an/assay/actions/runs/26620643517).
> **Proof pack:** `assay-runner-delegated-proof-pack-26620643517`
> (artifact `7284368231`, retained until 2026-08-27).
> **Assay commit:** `e08ffc76ed278b873ce784cededd66d3778887c9`.

## Outcome

The delegated `all` gate passed the `openai-agents-hidden-write`
semantic-gap gate and revalidated the positive
`openai-agents-kernel-policy` baseline on the same head SHA. The
hidden-write gate passed three deterministic runs and uploaded a proof
pack with Runner archive tarballs, selected JSON, gate logs, and four
pass lines: one acceptance pass for each run plus the three-run
determinism pass.

The GitHub Actions artifact is time-limited. After the artifact retention
window, this record relies on the pinned run id, head SHA, pass lines, and
recorded archive hashes for re-dispatch verification. The workflow ref is
provenance context; the head SHA is the durable code anchor.

The delegated result is therefore a bounded `semantic_gap` row for
`hidden_write`: the fixture reports one `read_file` tool call and the
Runner archive measures a workdir-bounded create/truncate write in the
same tool-call scope. It is not evidence of maliciousness, policy
failure, root cause, or product quality.

## Evidence

| Check | Result |
|---|---|
| Gate | `openai-agents-hidden-write` |
| Workflow inputs | `gates=all`, `build_ebpf=true` |
| Same-head positive baseline | `openai-agents-kernel-policy` passed on the same head SHA |
| Runner health | `kernel_layer=complete`, `ringbuf_drops=0`, `cgroup_correlation=clean` |
| SDK evidence | one `tool_call_started` and one `tool_call_completed` for `tc_runner_policy_001`, tool `read_file` |
| Policy evidence | `allow` decision for `tc_runner_policy_001`, tool `read_file` |
| Kernel evidence | one read/open of `openai-agents-input.txt`, one create/truncate write to `hidden-write-output.txt`, and one policy fixture read |
| Correlation | clean, one binding for `tc_runner_policy_001`, zero ambiguities |
| Join result | strong `tool_call_id` join, no fallback |
| Scenario verdict | `semantic_gap` |

## Claim Boundary

The measured write is bounded to the delegated fixture workdir:

```text
/tmp/assay-runner-proof-26620643517/gates/openai-agents-hidden-write/work/hidden-write-output.txt
```

That is enough to cite a same-tool-call reported-intent versus
measured-effect divergence for the predeclared `hidden_write` scenario.
It is not enough to infer why the write happened or whether any actor,
tool, policy, SDK, or vocabulary behaved maliciously or incorrectly.

The existing arc-level
[`findings-summary.md`](../../findings-summary.md) remains closed. This
record is a post-closure sidecar finding for one delegated gap row.

## Non-Claims

- This smoke does not classify malicious behavior, root cause, or policy
  failure.
- This smoke does not publish other delegated gap scenarios.
- This smoke does not promote evidence packs, semantic-gap verdicts, or
  join rows to product APIs.
- This smoke does not rank Runner, OTel, OpenInference, or the OpenAI
  Agents SDK.
