# TODO — Next Upstream Interop Lanes (2026 Q2)

- **Date:** 2026-04-08
- **Owner:** Evidence / Product
- **Status:** Active queue note. The `P11` commerce / trust-proof family is
  now formally started with [PLAN — P11A Visa TAP Intent Verification Evidence
  Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md), while
  Browser Use remains the active adjacent-space lane that should be finished
  cleanly before opening another new branch.
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

### Tier 1 — highest net priority

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 1 | `agno-agi/agno` | Already active | GitHub Discussion | `AccuracyEval` / `AccuracyResult` artifact | Cleanest same-space lane, eval-result-first, and socially much cleaner than frontier commerce lanes |

### Tier 2 — frontier, but heavier

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 2 | `P11A` — Visa TAP | Queued | GitHub issue | intent / auth verification result artifact | Most Assay-native frontier lane, but smaller ecosystem surface and higher semantic overclaim risk than Agno |

### Tier 3 — highest upside adjacent

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 3 | `browser-use/browser-use` | Active planning | GitHub Discussion | `AgentHistoryList` / `action_history()` / `final_result()` / `errors()` | Strong adjacent-space momentum and a history/output seam that is clearly different from prior trace and eval lanes |

### Tier 4 — strategically strong, socially harder

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 4 | `langfuse/langfuse` | Queued | GitHub Discussion | dataset / experiment result export or score record export | Strategically strong, but most likely to read as platform-on-platform if the framing slips |

### Tier 5 — frontier, but not first

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 5 | `P11B` — x402 | Queued | publish / integrate first | payment lifecycle evidence | Technically exciting, but the channel is weak and payment semantics are much riskier than TAP |

### Tier 6 — good lanes, less urgent

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 6 | `mastra-ai/mastra` | Queued | GitHub issue | `evaluate()` / scorer result / CI eval result | Good candidate, but weaker channel than Agno and less distinctive than Browser Use |
| 7 | `pydantic/pydantic-ai` | Already active | GitHub issue | `EvaluationReport`-derived artifact | Strong lane, but less urgent now that it is already live and Agno covers the same broad eval-result family |
| 8 | `lastmile-ai/mcp-agent` | Already active | GitHub Discussion | token summary / runtime accounting | Strong lane, but more runtime-accounting-specific than the next frontier or adjacent priorities |

### Tier 7 — later / watchlist

| Rank | Repo / lane | Status | Primary channel | First seam | Why it ranks here |
|------|-------------|--------|-----------------|------------|-------------------|
| 9 | `P11C` — Identus | Watchlist | GitHub Discussion | credential / delegation proof | Interesting, but heavier and more infrastructural than TAP |
| 10 | `openlit/openlit` | Watchlist | GitHub Discussion | eval/export or score record export | Worth keeping as a special-case OTel-native candidate, but not the best next general lane |
| 11 | `livekit/agents` | Watchlist | issue or discussion only if a small hook surface becomes clear | metrics / event hook evidence at most | Lower fit because the public seam is much more deployment and observability heavy than artifact-first |
| 12 | `microsoft/autogen` | Deprioritized | GitHub Discussion | n/a | Keep low because the repo is explicitly in maintenance mode |

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

## 5. Why Browser Use is now the active lane to finish

`browser-use/browser-use` is not the highest strategic priority overall, but it
is the right **active adjacent lane to finish first** because:

- the planning slice is already in progress
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

The active Browser Use plan now lives in
[PLAN — P12 Browser Use History / Output Evidence Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md).

## 6. Why `P11A` stays above Browser Use in the broader ranking

The `P11A` Visa TAP lane stays above Browser Use in the broader ranking because
it has stronger frontier value:

- verification-first rather than platform-first
- cryptographic and protocol-adjacent enough to fit Assay's trust-compiler
  direction closely
- strategically different from another framework or eval lane

Why it still is **not** the active lane right now:

- issue-only channel
- smaller ecosystem surface
- much faster semantic overclaim risk
- more explanation required per word

That is why `P11A` remains the better frontier priority while Browser Use
remains the better lane to finish first.

The formal frontier plan now lives in
[PLAN — P11A Visa TAP Intent Verification Evidence
Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md).

## 7. Why Langfuse stays below Browser Use

`langfuse/langfuse` remains strategically important, but it is still not the
best immediate next move after Browser Use.

Why:

- Langfuse already positions itself as a broad LLM engineering platform with
  observability, datasets, scores, and experiments
- that makes the seam real
- but it also makes the outreach socially riskier because Assay can be read as
  another platform talking to a platform

The right posture there is:

- export/import sample first
- score or dataset/experiment result seam
- only then one small GitHub Discussion

That is still a good lane, just not the safest next one.

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
- Langfuse should stay export/import-first, not trace-first
- Mastra should stay eval-result-first, not tracing-first
- OpenLIT should remain a special-case OTel-native candidate, not the default
  next lane

## 10. Next actions

1. **Now active:** finish [PLAN — P12 Browser Use History / Output Evidence
   Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md).
2. After the Browser Use sample lands and the outward Browser Use question is
   posted, let that lane breathe before opening another new outward thread.
3. Decide next between:
   - executing `P11A` if frontier / protocol depth is the next deliberate move
   - `Langfuse` if export/import platform adjacency is the next deliberate move
4. Keep **Mastra** as the main fallback if the Langfuse positioning risk feels
   too high at that time.

## References

- [PLAN — P10 Agno Accuracy Eval Evidence Interop](./PLAN-P10-AGNO-ACCURACY-EVAL-EVIDENCE-2026q2.md)
- [PLAN — P11A Visa TAP Intent Verification Evidence Interop](./PLAN-P11A-VISA-TAP-INTENT-VERIFICATION-EVIDENCE-2026q2.md)
- [PLAN — P12 Browser Use History / Output Evidence Interop](./PLAN-P12-BROWSER-USE-HISTORY-OUTPUT-EVIDENCE-2026q2.md)
