# TODO — Next Upstream Interop Lanes (2026 Q2)

- **Date:** 2026-04-08
- **Owner:** Evidence / Product
- **Status:** Active queue note. The `P11` commerce / trust-proof family is
  now formally started with [PLAN — P11A Visa TAP Intent Verification Evidence
  Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md), the
  Browser Use adjacent lane is now live, and
  [PLAN — P13 Langfuse Experiment Result Evidence Interop](./PLAN-P13-LANGFUSE-EXPERIMENT-RESULT-EVIDENCE-2026q2.md)
  is the next planned platform-adjacent candidate.
- **Scope (this document now):** Record the ranked post-Agno queue for the
  next upstream interop lanes, the reasons behind that ordering, and the
  execution rules learned from the current wave.

## 1. Why this queue exists

After the current framework, protocol, runtime-accounting, and eval-report
wave, Assay now has enough signal to rank the next outreach candidates more
strictly.

The working filter on 2026-04-08 was:

- current repo momentum
- one official seam already documented upstream
- a natural maintainer channel
- a high chance that sample-first outreach will read as a technical boundary
  question, not as promotion

This note is the queue record for that ranking and the resulting execution
order.

It is not an implementation PR.

It is not an outward post.

It is not a commitment to run every candidate below immediately.

## 2. What changed after the current wave

The current wave produced a few useful operating rules:

- **GitHub-native outreach works better than forum-first outreach.**
  The LangGraph moderation hold is the clearest counterexample.
- The strongest lanes are the ones where upstream already documents one small
  official seam.
- Trace-first is no longer the default choice just because a repo exposes
  observability.
- Platform-adjacent tools require a different posture from framework repos.
  Import/export slices are usually safer than leading with a generic
  observability pitch.

Those lessons now drive the queue below.

## 3. Overall priority list

### Tier 0 — first finish what is already live

| Rank | Lane | Status | Why it stays first |
|------|------|--------|--------------------|
| 0 | Current open wave | Active | Let the merged samples, live threads, and held-back UCP lane settle before forcing new surface area |

Tier 0 means:

- no outward UCP post
- no extra pushes on cold threads
- keep current live lanes breathing unless an upstream maintainer responds

### Tier 1 — next best active candidate

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 1 | `langfuse/langfuse` | Active planning | GitHub Discussion (`Support`) | bounded experiment item result / evaluation export | Best next fit after Browser Use and TAP: strong repo momentum, natural maintainer channel, and a smaller eval-result seam than trace-first observability |

### Tier 2 — clean fallback if Langfuse framing risk rises

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 2 | `mastra-ai/mastra` | Queued | GitHub issue | `evaluate()` / scorer result / CI eval result | Good candidate and less platform-on-platform risky than Langfuse, but weaker channel shape because the repo has no Discussions |

### Tier 3 — special-case OTel-native candidate

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 3 | `openlit/openlit` | Watchlist | GitHub Discussion | eval/export or bounded score record export | Worth keeping as the main OTel-native special case, but still not the best general next lane |

### Tier 4 — later frontier and heavier infra lanes

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 4 | `P11B` — x402 | Queued | publish / integrate first | payment lifecycle evidence | Technically interesting, but the repo currently has no Issues or Discussions and the semantics are much riskier than TAP |
| 5 | `P11C` — Identus | Watchlist | GitHub Discussion | credential / delegation proof | Interesting, but heavier and more infrastructural than the next eval/export lane |

### Tier 5 — still lower fit

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 6 | `livekit/agents` | Watchlist | issue or discussion only if a small hook surface becomes clear | metrics / event hook evidence at most | Lower fit because the public seam is much more deployment and observability heavy than artifact-first |
| 7 | `microsoft/autogen` | Deprioritized | GitHub Discussion | n/a | Keep low because the repo is explicitly in maintenance mode |

## 4. Historical note on Agno

At the time of discovery, `agno-agi/agno` ranked first among the same-space
framework candidates because:

- Discussions were enabled
- Evals and Tracing were clearly separated in the docs
- `AccuracyEval` was a cleaner first seam than another trace-export sample

That choice has now already been executed in the current wave. The formal plan
is [PLAN — P10 Agno Accuracy Eval Evidence Interop](./PLAN-P10-AGNO-ACCURACY-EVAL-EVIDENCE-2026q2.md).

That lane is already in motion, so the queue no longer starts with Agno even
though Agno remains the strongest general-purpose next-lane choice in the
ranking.

## 5. Historical note on Browser Use

`browser-use/browser-use` was not the highest strategic priority overall, but
it was the right adjacent lane to finish before opening the next platform lane
because:

- the planning slice was already in progress
- the seam is clean and materially different from the current wave
- it can be finished without opening the heavier `P11A` commerce branch yet
- it preserves one-lane-at-a-time discipline better than pivoting mid-plan

The critical Browser Use lesson is that the best seam is **not** observability.

The docs expose:

- `AgentHistoryList`
- `action_history()`
- `final_result()`
- `errors()`
- `structured_output`

At the same time, Browser Use also documents Laminar, OpenLIT, and telemetry.
That broader observability layer is exactly what Assay should avoid as the
first wedge.

That lane is now live. The formal Browser Use plan lives in
[PLAN — P12 Browser Use History / Output Evidence Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md).

## 6. Historical note on `P11A`

The `P11A` Visa TAP lane was ranked above Browser Use in the broader frontier
ordering because it had stronger protocol value:

- verification-first rather than platform-first
- cryptographic and protocol-adjacent enough to fit Assay's trust-compiler
  direction closely
- strategically different from another framework or eval lane

The formal frontier plan now lives in
[PLAN — P11A Visa TAP Intent Verification Evidence
Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md).

That lane is now live too, so the queue no longer needs to choose between
Browser Use and `P11A` as the next move.

## 7. Why Langfuse is now the next planned lane

`langfuse/langfuse` is now the next best planned lane because the two lanes
that previously sat ahead of it in execution order are already in motion.

Why it now moves up:

- strong repo momentum
- Discussions enabled with an answerable `Support` category
- official eval docs around datasets, experiments, and scores
- API-first and export-friendly positioning
- a seam that is different from Browser Use history/output and TAP
  verification

Why it is still socially harder than the earlier framework lanes:

- Langfuse already positions itself as a broad LLM engineering platform with
  observability, datasets, scores, and experiments
- that makes the seam real
- but it also makes the outreach socially riskier because Assay can be read as
  another platform talking to a platform

The right posture there is:

- export/import sample first
- bounded experiment-result seam first
- `Support` Discussion only after the sample lands
- no trace-first framing

The formal Langfuse plan now lives in
[PLAN — P13 Langfuse Experiment Result Evidence Interop](./PLAN-P13-LANGFUSE-EXPERIMENT-RESULT-EVIDENCE-2026q2.md).

## 8. Why Mastra stays below the top four

`mastra-ai/mastra` is slightly lower in strategic weight than Langfuse, but in
some respects cleaner to approach because it has less platform-on-platform
friction.

The main reason it stays below the top four in this queue is channel shape:

- no Discussions
- outward route is issue-first

That means the lane may eventually be easier socially, but it is less natural
as the next GitHub-native sample-backed question.

## 9. Sequencing rules

The queue should be executed under the same discipline as the current wave:

1. one repo at a time
2. sample first
3. one small outward move only after the sample lands on `main`
4. no second seam in the first sample
5. no observability-first pitch when a smaller result artifact exists

Additional queue rules:

- reserve `P11` for the commerce / trust-proof family
- Browser Use should stay output/history-first, not Laminar/OpenLIT-first
- `P11A` should stay verification-first, not payment-truth-first
- Langfuse should stay experiment-result-first, not trace-first
- Mastra should stay eval-result-first, not tracing-first
- OpenLIT should remain a special-case OTel-native candidate, not the default
  next lane

## 10. Next actions

1. **Now active:** formalize
   [PLAN — P13 Langfuse Experiment Result Evidence Interop](./PLAN-P13-LANGFUSE-EXPERIMENT-RESULT-EVIDENCE-2026q2.md).
2. Let the fresh Browser Use and Visa TAP outward threads breathe unless an
   upstream maintainer responds.
3. If no hot follow-up overrides the queue, build the `P13` sample next.
4. Keep **Mastra** as the main fallback if the Langfuse positioning risk feels
   too high once implementation starts.

## References

- [PLAN — P10 Agno Accuracy Eval Evidence Interop](./PLAN-P10-AGNO-ACCURACY-EVAL-EVIDENCE-2026q2.md)
- [PLAN — P11A Visa TAP Intent Verification Evidence Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md)
- [PLAN — P12 Browser Use History / Output Evidence Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md)
- [PLAN — P13 Langfuse Experiment Result Evidence Interop](./PLAN-P13-LANGFUSE-EXPERIMENT-RESULT-EVIDENCE-2026q2.md)
