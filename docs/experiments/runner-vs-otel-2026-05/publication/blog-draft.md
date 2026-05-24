# Replay archives next to live traces: a shape-by-shape comparison of agent observability

> **Status:** draft. Not yet published. Lives in the repo so the
> framing matches the evidence on disk and so reviewers can
> verify every number from the committed artefacts.
>
> **Date written:** 2026-05-25.

OpenTelemetry GenAI semantic conventions, OpenInference, traceAI,
Phoenix, Langfuse, Pydantic Logfire — the AI observability world
in 2026 is rich and converging on a shared idea: capture what the
agent *reports* doing as structured spans, ship the spans through
familiar OTel infrastructure, and reason about agent behaviour
through that lens. The frontier most people are talking about is
prompt-and-completion semantics, span/event/metric carrier
distinctions, and how to do tool-call argument capture without
leaking sensitive content.

This post is about a different cut of the same agent run.

Internally we maintain a Linux/eBPF measured-run capture called
Assay-Runner. It produces a content-addressed measured-run
archive of an agent execution (per-run tamper-evident binding,
shape stability across runs, not byte-identical determinism):
cgroup-scoped kernel events,
normalized policy decisions, and SDK tool-call events bound by
the same `tool_call_id` an OTel trace would carry. It is internal
to a sibling project (`Rul1an/assay`), Linux-only, no public users,
not a tracing tool, not a competitor to anything OTel covers.

The question that kept coming up: **what happens if you capture the
*same* agent run both ways and put the artefacts side by side?**
Not "which is better" — that question is uninteresting. The interesting
question is which claims each layer can support, where the two
layers complement each other, and where they say different things
about the same execution.

So we built the experiment, committed every artifact, and now I
have something to write up. Everything in this post is verifiable
from
[`docs/experiments/runner-vs-otel-2026-05/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05)
in the assay repo — including a stdlib-only Python comparator
(`compare/compare.py`) that takes a trace + an archive and
produces the field matrix shown below.

## TL;DR for tracing folks

OTel/OpenInference traces do exactly what they say: they capture
the agent's reported control flow with semantic clarity. They are
not designed to bound what the system actually did at the kernel
level. That's fine. The interesting move is to **pair** a trace
with a content-addressed runtime-evidence artifact that *is*
designed to bound system effects, and let each layer make the
claims it was built for.

This experiment shows three things on real Linux/eBPF data, all
verifiable from the committed artefacts:

1. **Per-run tamper-evident binding works.** An
   `assay.archive.created` span event carrying
   `sha256:<manifest-bytes>` lets a consumer cryptographically pair
   a trace with exactly one runtime-evidence archive.
2. **Tool-level join works** via shared `tool_call_id` between
   `gen_ai.tool.call.id` on the trace and the SDK-event
   `tool_call_id` in the archive.
3. **Reported intent ≠ measured effect** is *expressible* once
   binding + tool-level join hold: when the agent reports reading
   file X and the kernel observes a read of file Y at the same
   `tool_call_id`, the comparator surfaces the asymmetry as
   `intent-effect-mismatch:<path>`.

What it does **not** show:

- It does not say OTel is missing anything. The trace is honest
  about what the agent reported.
- It does not claim byte-identical determinism across live eBPF
  runs (run id, timestamps, PIDs, inodes vary; only shape is
  stable).
- It does not propose a new tracing stack. It proposes — at most —
  a small vocabulary addition so existing traces can refer to
  out-of-band runtime evidence by digest.

## The setup

One deterministic workload: an `@openai/agents` agent with a
cassette-style local provider that produces a single
`read_file` tool call with a fixed `tool_call_id`
(`tc_runner_policy_001`). No network. Cross-platform for the
trace-only arm; Linux/eBPF for the dual-capture arm.

Three arms:

| Arm | Trace | Archive | Where |
|---|---|---|---|
| A — Runner only | no | yes | Linux/eBPF (delegated host) |
| B — Trace only | yes | no | macOS / Linux / Windows |
| C — Dual capture | yes | yes | Linux/eBPF (delegated host) |

The comparator (`compare/compare.py`) reads `(archive, trace)` and
emits a 17-row field matrix and a top-level summary:

```
trace spans: 2
archive SDK events: 3
manifest-digest binding: tamper-evident-match
tool_call_id join: joined:tc_runner_policy_001
intent-vs-effect: intent-effect-match | intent-effect-mismatch:<path> | not-applicable
```

The `intent-vs-effect` field is what makes the experiment
publishable. It distinguishes "the agent reported what it did"
from "the kernel agrees with what the agent reported" and reports
the disagreement when there is one.

## What L1 alone can carry (and what it can't)

In the Arm B baseline (three identical runs, see
[`runs/v1-baseline/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05/runs/v1-baseline)):

| Field | OTel/OpenInference trace |
|---|---|
| `gen_ai.provider.name` | `openai` |
| `gen_ai.tool.name` | `read_file` |
| `gen_ai.tool.call.id` | `tc_runner_policy_001` |
| `gen_ai.usage.input_tokens` | absent here (cassette model doesn't set it; a real model call would populate) |
| Filesystem effects | n/a — out of contract |
| Network effects | n/a — out of contract |
| Measurement integrity | n/a — out of contract |

A trace cannot bound system effects because it was never asked to.
That is not a flaw; it is the right separation. Asking a trace to
prove a negative ("nothing else happened") is the wrong question
for the abstraction.

## What L2 adds: bounded negatives + measurement health

In the Arm C baseline (three iterations under real Linux/eBPF,
see
[`runs/v1-arm-c/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05/runs/v1-arm-c)):

| Field | Runner archive |
|---|---|
| `capability_surface.filesystem_paths` | `workload.js`, `otel-setup.js`, `manifest-binding.js`, `package.json`, fixture file, trace.json write — the actual file opens the Node runtime made |
| `observation_health.kernel_layer` | `complete` |
| `observation_health.ringbuf_drops` | `0` (hard gate) |
| `observation_health.cgroup_correlation` | `clean` |
| `correlation_report.status` | `clean` |
| Per-run manifest digest | `sha256:<bytes>` — different bytes each run because timestamps/PIDs vary |

On Linux with healthy capture this lets you say "within this
measurement boundary, X did not happen" — a *bounded negative*.
Bounded negatives are the asymmetry that motivates the experiment.
A trace can say "we did Y"; an archive can say "we did Y, and we
did *not* do W either."

A subtlety we landed on the honest way: live-eBPF archives are
not byte-identical across runs. Same workload, same
`require_binding_match` exit 0, same field shape, same line
counts — different bytes, because the kernel measures real
timestamps/PIDs/inodes. The right published claim is
**per-run** tamper-evident binding plus **shape** stability, not
cross-run byte determinism.

## The binding

```typescript
// workload/src/otel-setup.ts (excerpt)
await tracer.startActiveSpan("assay.runner.measured_run", async (span) => {
  // ... do the agent run ...

  // Post-run: bind the trace to the just-written archive by digest.
  const manifestBytes = readFileSync(extractedManifestPath);
  const digest = `sha256:${createHash("sha256").update(manifestBytes).digest("hex")}`;

  span.addEvent("assay.archive.created", {
    "assay.archive.schema": "assay.runner.archive_manifest.v0",
    "assay.archive.manifest_digest": digest,
    "assay.archive.path": archivePath,
    "assay.runner.ringbuf_drops": 0,
    "assay.runner.correlation_status": "clean",
  });
});
```

That's it on the trace side. On the archive side the same
`manifest.json` bytes hash to the same digest. The comparator
checks them and reports `tamper-evident-match` (or `mismatch` or
the various "absent" states). With `--require-binding-match`, exit
code 3 if the bytes don't line up.

This is the SLSA provenance pattern, applied at runtime. A small
structured record refers to an artefact by digest rather than
embedding the artefact itself.

## The tool-level join

OTel GenAI semconv defines `gen_ai.tool.call.id` on tool spans.
The runner archive's SDK layer stores the same id in its
`sdk_event.tool_call_id`. When you arrange the workload to write
both (one to the OTLP file exporter, one to the
`$ASSAY_RUNNER_SDK_EVENT_LOG` NDJSON that the runner folds into
the archive's `layers/sdk.ndjson`), the comparator joins them by
exact id match.

Slice 2 of the experiment series lands this end-to-end. After it,
the `runs/slice2-arm-c/` baseline reports
`tool_call_id join: joined:tc_runner_policy_001` on real eBPF
data — the first proof that the L1 trace and the L2 archive
agree on which tool call we're talking about.

## The interesting one: intent vs effect

With tool-level join holding and the trace optionally carrying
`gen_ai.tool.call.arguments` (sensitive content, opt-in), the
comparator can compare what the agent *reported* doing to what
the kernel *measured* it doing — at the same `tool_call_id`.

Slice 3 of the experiment series demonstrates this on a
controlled adversarial scenario. The deterministic model reports
reading `${workdir}/agent-claimed-fixture.txt`; the workload
never creates that file; the `read_file` tool implementation
under `--tampering` mode ignores the reported path and reads
`${workdir}/tampering-target.txt` instead.

Three Arm C iterations on `assay-bpf-runner` (see
[`runs/slice3-arm-c/`](https://github.com/Rul1an/assay/tree/main/docs/experiments/runner-vs-otel-2026-05/runs/slice3-arm-c))
report, identically:

```
intent_effect_status: intent-effect-mismatch:<workdir>/agent-claimed-fixture.txt
```

- The trace carries
  `gen_ai.tool.call.arguments = {"path": "<workdir>/agent-claimed-fixture.txt"}`
- The archive's `capability_surface.filesystem_paths` includes
  `tampering-target.txt` (the redirected read) and the workload's
  own JS file opens.
- `agent-claimed-fixture.txt` is **nowhere** in
  `capability_surface` — the kernel never recorded that open, so
  the agent never executed it.
- `tool_call_id_join` still works
  (`joined:tc_runner_policy_001`); both sides agree on which
  tool call we are talking about.
- `manifest_digest_binding` still works
  (`tamper-evident-match`); each trace still cryptographically
  binds to its own archive.

The reported intent is divorced from the measured effect, the
divergence is anchored at the same `tool_call_id`, and the
artefact pair is tamper-evident. **That is the central
asymmetry claim, demonstrable on real Linux/eBPF data.**

## What I'm asking the OpenInference / OTel community

Not adoption. Not integration. Just a vocabulary question.

The binding event and the intent-vs-effect status feel like they
could be useful in any setup that pairs an OTel-family trace
with an out-of-band runtime-evidence artifact, not just ours.
We've parked them in a private `assay.*` namespace because we
didn't want to squat on unspecified OTel GenAI / OpenInference
names.

Four candidate attribute names:

- `agent.runtime_evidence.digest` — content-addressed pointer
- `agent.runtime_evidence.health` — measurement integrity status
- `agent.runtime_evidence.boundary` — declared measurement boundary
- `agent.runtime_evidence.intent_effect_status` — reported vs measured verdict

The discussion draft asking for placement guidance lives in the
repo at
[`publication/openinference-discussion.md`](https://github.com/Rul1an/assay/blob/main/docs/experiments/runner-vs-otel-2026-05/publication/openinference-discussion.md).

## Non-claims (in case anyone reading this thinks I'm selling something)

- I'm not claiming OTel traces are missing anything they were
  supposed to carry.
- I'm not proposing a new tracing stack or a new vendor.
- I'm not claiming the runtime-evidence layer is novel — eBPF +
  agent observability is an active research area (AgentSight et
  al.). What's new is the per-run cryptographic binding to an
  OTel-family trace and the intent-vs-effect status expressed at
  the same `tool_call_id`.
- I'm not claiming Linux-only is a permanent limitation; it is
  the current measurement boundary of the prototype runtime.
- I'm not running this as a product. The four Assay-Runner
  crates that publish to crates.io
  (`assay-runner-{schema, core, linux, spike}`) exist there only
  so `assay-cli` and its workspace dependencies stay resolvable;
  each crate's description carries explicit
  internal/experimental framing
  (`No standalone product guarantee; API surface remains narrow
  and intentionally undocumented for third-party use; semver
  tracks the Assay workspace`). See the v3.11.3 CHANGELOG entry
  for why the crates are publishable and what that does and does
  not commit us to.

If the four candidate attribute names land in someone else's
vocabulary, that's a good outcome and we move the experiment's
implementation to use them. If they don't, the experiment's
evidence still holds; only the namespace is uncertain.

## Reproduce

```bash
# Clone
git clone https://github.com/Rul1an/assay.git
cd assay

# Local Arm B (trace-only, macOS/Linux/Windows)
cd docs/experiments/runner-vs-otel-2026-05/workload
npm install --no-audit --no-fund --ignore-scripts
npx tsc -p tsconfig.json
node dist/workload.js --run-id "demo" --trace-out "/tmp/trace.json"

# Comparator + Arm C verification run from the experiment package root
cd ..
python3 compare/compare.py \
  --archive compare/tests/fixtures/archive \
  --trace /tmp/trace.json --out-md /tmp/matrix.md
cat /tmp/matrix.md

# Verify the committed Arm C baselines (Linux/eBPF runs)
for d in runs/slice3-arm-c/run_arm_c_*; do
  python3 compare/compare.py \
    --archive "$d/archive-contents" --trace "$d/trace.json" \
    --require-binding-match >/dev/null && \
    echo "$(basename $d) OK"
done

# All three print OK; matrix.json's
# summary.intent_effect_status reads
# intent-effect-mismatch:<workdir>/agent-claimed-fixture.txt
```

The full experiment package is committed; each per-run artifact
directory contains the trace, the comparator output, and the
extracted archive contents (manifest, capability surface,
observation health, correlation report, ndjson layers). Raw
`.tar.gz` archives are not tracked because they don't add anything
verifiable that the extracted form doesn't already give a
reviewer.

---

*Side note for tracing tool maintainers: if you read this and
think "this should live as a span attribute X under spec Y", the
OpenInference discussion linked above is where I'd most like to
hear that. Single narrow question, one cross-link to here for
evidence, no follow-up volley.*
