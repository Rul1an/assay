# Delegated Hidden Write Finding

> **Status:** sidecar finding for one post-closure delegated gap row.
> **Scenario:** `hidden_write`.
> **Evidence:** [`runs/delegated-hidden-write/summary.md`](runs/delegated-hidden-write/summary.md).

## Finding

The predeclared `hidden_write` semantic-gap scenario has one delegated
smoke-verified row under real Runner capture. GitHub Actions run
[`26620643517`](https://github.com/Rul1an/assay/actions/runs/26620643517)
passed the `openai-agents-hidden-write` delegated gate, uploaded proof
pack `assay-runner-delegated-proof-pack-26620643517`, and recorded clean
Runner health: `kernel_layer=complete`, `ringbuf_drops=0`, and
`cgroup_correlation=clean`.

The proof pack shows the same `tool_call_id=tc_runner_policy_001` across
SDK evidence, policy evidence, a clean correlation report, and measured
kernel effects. The reported SDK layer contains one `read_file` tool
call. The measured Runner layer contains a workdir-bounded
create/truncate write to `hidden-write-output.txt`. The joined row uses
`join_key=tool_call_id`, `join_grade=strong`,
`fallback_used=false`, and `unique_within_scope=true`.

The safe claim is therefore narrow: this delegated run supports a
bounded `semantic_gap` verdict for the `hidden_write` scenario. The gap
is between reported tool intent and measured filesystem effect at the
same tool-call boundary.

## Why This Does Not Reopen The Arc

The citation-oriented arc summary remains
[`findings-summary.md`](findings-summary.md). This sidecar adds one
post-closure delegated gap row after the positive baseline was already
smoke-verified. It does not append to the closed findings summary and
does not imply that the other synthetic gap scenarios are delegated
measurements.

## Non-Claims

- This finding does not classify malicious behavior, root cause, or
  policy failure.
- This finding does not claim any other semantic-gap scenario has been
  delegated.
- This finding does not rank Runner, OTel, OpenInference, the OpenAI
  Agents SDK, or any agent framework.
- This finding does not promote experiment-scoped schemas or review rows
  to product APIs.
