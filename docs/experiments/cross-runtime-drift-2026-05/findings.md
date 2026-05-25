# Cross-Runtime Drift — findings (live n=3 baseline)

> **Status:** live baseline committed. The Slice 3 workflow
> [`Cross-Runtime Drift Experiment`](../../../.github/workflows/cross-runtime-drift-experiment.yml)
> ran successfully on `assay-bpf-runner` as
> [GitHub Actions run 26398427430](https://github.com/Rul1an/assay/actions/runs/26398427430)
> on head `e3f6ef9d`, with `repetitions=3` and `build_ebpf=true`.
> Arm A (`@openai/agents`) baselines are under `runs/a0/`,
> Arm B (`@google/genai`) baselines are under `runs/b0/`,
> and per-pair drift reports are under `runs/drift/`.
>
> **Last updated:** 2026-05-25.
>
> **Plan-doc:** [`../cross-runtime-drift-2026-05.md`](../cross-runtime-drift-2026-05.md)
> — research question, drift dimensions, threats to validity,
> sequencing.

## TL;DR

The same workload contract was executed under two runtimes:

- Arm A: `openai-agents`, model `gpt-4o-mini`
- Arm B: `gemini-genai`, model `gemini-2.5-flash`

All six captures passed the measurement-health gate:
`kernel_layer=complete`, `ringbuf_drops=0`,
`cgroup_correlation=clean`, `correlation_report.status=clean`,
and `sdk_layer=self_reported`. The workflow contract-checker also
passed before artifacts were uploaded.

Across all three live archive pairs, the drift labels were stable:

| Dimension | Live classification | Stability | Short read |
|---|---|---|---|
| `filesystem_paths_touched` | **runtime-induced** | 3/3 | Each arm touched its own run-local SDK/tool-call files and workload package metadata; shared `/etc/*` resolver config appeared in both. |
| `kernel_file_operations` | **runtime-induced** | 3/3 | New open metadata splits successful `openat`/`openat2` events into `read`, `write`, `create`, `truncate`, and `append` operation strings. |
| `network_endpoints` | **runtime-induced** under v0 | 3/3 | v0 records IP endpoints, not provider hostnames. The comparator cannot map these back to OpenAI/Google hostnames, so the non-shared IPs remain unclassified and the row lands runtime-induced. |
| `process_execs` | **inconclusive** | 3/3 | Empty in both arms under capability_surface v0. The workload process itself is not represented as a child exec. |
| `sdk_tool_events` | **task-induced** | 3/3 | Both arms emitted SDK events for `read_file` and `write_file`. |
| `mcp_tool_surface` | **inconclusive** | 3/3 | Empty in both arms; the workload contract forbids MCP servers. |
| `tool_invocation_order` | **task-induced** | 3/3 | Both arms invoked `read_file -> write_file` in order. |

Summary per pair: **2 task-induced, 3 runtime-induced,
2 inconclusive, 0 provider-induced**.

That differs from the synthetic fixture, which deliberately exercised
all four labels, including `provider-induced`. The live run teaches a
useful v0 boundary: provider-host classification needs hostnames or a
DNS attribution layer. With IP-only `capability_surface.network_endpoints`,
the comparator correctly refuses to guess.

## Live drift table

The three reports under [`runs/drift/`](runs/drift/) are stable enough
to summarize once. Pair-specific counts are preserved below where they
vary.

| Dimension | Source | Pair 1 | Pair 2 | Pair 3 | Interpretation |
|---|---|---:|---:|---:|---|
| `filesystem_paths_touched` | `capability_surface.filesystem_paths` | runtime-induced, 18 non-shared | runtime-induced, 18 non-shared | runtime-induced, 18 non-shared | Runtime/run-local files differ by arm. Shared resolver config appears in both. |
| `kernel_file_operations` | `layers/kernel.ndjson` open metadata | runtime-induced, 42 non-shared | runtime-induced, 42 non-shared | runtime-induced, 42 non-shared | Operation-aware kernel rows now distinguish `read`, `write`, `create`, `truncate`, and `append` for successful opens. |
| `network_endpoints` | `capability_surface.network_endpoints` | runtime-induced, 19 non-shared | runtime-induced, 18 non-shared | runtime-induced, 18 non-shared | OpenAI/Gemini traffic appears as IP endpoints. Provider attribution is not possible from v0 hostless data. |
| `process_execs` | `capability_surface.process_execs` | inconclusive | inconclusive | inconclusive | Empty in both arms. |
| `sdk_tool_events` | `layers/sdk.ndjson` tool field | task-induced | task-induced | task-induced | Full overlap: `read_file`, `write_file`. |
| `mcp_tool_surface` | `capability_surface.mcp_tools` | inconclusive | inconclusive | inconclusive | Empty in both arms by contract. |
| `tool_invocation_order` | `layers/sdk.ndjson` start events ordered by `seq` | task-induced | task-induced | task-induced | Identical sequence: `read_file -> write_file`. |

## Per-dimension narrative

### Filesystem drift is real but narrow

Each live pair has nine Arm-A-only paths and nine Arm-B-only paths.
They are not arbitrary application files; they are run-local or
runtime-specific support files:

- `.../arm-a-openai-runs/<run>/sdk-events.ndjson`
- `.../arm-a-openai-runs/<run>/workdir/fixture-input.txt`
- `.../arm-a-openai-runs/<run>/workdir/fixture-output.txt`
- `.../arm-a-openai-runs/<run>/workdir/run-meta.json`
- `.../arm-a-openai-runs/<run>/workdir/tool-calls.ndjson`
- `.../workload-openai/dist/sdk-events.js`
- `.../workload-openai/dist/workload.js`
- `.../workload-openai/dist/package.json`
- `.../workload-openai/package.json`
- the equivalent Gemini paths under `arm-b-gemini-runs/` and
  `workload-gemini/`

The seven paths shared by both arms are host resolver configuration:
`/etc/gai.conf`, `/etc/host.conf`, `/etc/hosts`, `/etc/netsvc.conf`,
`/etc/nsswitch.conf`, `/etc/resolv.conf`, `/etc/svc.conf`.

The live claim is therefore modest and useful: under the same Runner
boundary and same workload contract, the two runtime arms expose a
different touched-path surface, but in this v0 run the difference is
small and dominated by run-local/runtime package plumbing.

### Kernel file operations now split read/write/create/truncate

This rerun includes Runner kernel-event open metadata: `flags`, `mode`,
`resolve`, `return_value`, derived `access_mode`, derived
`operation_flags`, and success/error `status`. The drift comparator now
uses `layers/kernel.ndjson` to project successful open events into
operation strings such as:

- `read:<workdir>/fixture-input.txt`
- `write:<workdir>/fixture-output.txt`
- `create:<workdir>/fixture-output.txt`
- `truncate:<workdir>/fixture-output.txt`

Across all three live pairs, both arms show the same logical fixture
operation shape, but the absolute run directories differ by arm. The row
therefore still lands `runtime-induced`: the comparator sees
arm-local paths, not a normalized task alias. That is an honest v0
boundary, not a missing signal. The useful change is that the archive now
contains enough kernel metadata to avoid the older "touched path only"
claim.

### Network drift is the sharpest v0 limitation

The synthetic fixture expected provider-host classification
(`api.openai.com`, `generativelanguage.googleapis.com`,
`oauth2.googleapis.com`). The live capture records IP endpoints instead:
Cloudflare-owned IPs on the OpenAI arm, Google-owned IPs on the Gemini
arm, plus shared local resolver traffic at `127.0.0.53:53`.

`compare/drift.py` intentionally does not reverse-map IP addresses to
providers. That would be temporal, DNS-dependent, and outside the
archive contract. Therefore the live `network_endpoints` row lands
`runtime-induced` mechanically: non-shared items exist, none match the
provider-host whitelist, none are fixture paths.

This is not a claim that the network drift is *semantically* caused by
the runtime rather than provider transport. It is a claim that
capability_surface v0 does not carry enough host attribution to label
the row provider-induced. A v2 comparator could add DNS/hostname
binding if the archive starts carrying that evidence.

### SDK rows are the contract success signal

Both workloads wrote five SDK events per run:

1. `tool_call_started` for `read_file`
2. `tool_call_completed` for `read_file`
3. `tool_call_started` for `write_file`
4. `tool_call_completed` for `write_file`
5. `run_finished`

The comparator deduplicates the tool field for `sdk_tool_events` and
orders `tool_call_started` events for `tool_invocation_order`. Both rows
are task-induced across all three pairs. That is the intended result:
the workload contract pinned the logical tool surface and sequence, and
the two runtimes satisfied it.

### Empty process and MCP rows are honest inconclusive rows

`process_execs` is empty in both arms. Under this v0 capture, the Node
workload process is the captured child, not a child process spawned by
the workload. The row is therefore not evidence that "no process
execution happened"; it is evidence that this dimension is empty under
the current capability_surface projection.

`mcp_tool_surface` is also empty in both arms because the workload
contract forbids MCP servers. This is expected-inconclusive. A future
variant that registers a deliberate MCP tool would be needed to turn
this row into a real signal.

## What the live captures add over the synthetic baseline

The synthetic fixture remains useful as a comparator smoke test because
it exercises every classification label exactly once. The live baseline
adds four things the synthetic fixture could not:

1. **Measurement health under real eBPF capture.** All six archives have
   `ringbuf_drops=0`, clean cgroup correlation, and complete kernel
   capture.
2. **Real SDK-layer ingestion in both runtimes.** `sdk_layer=self_reported`
   and five SDK events per run prove the Runner archive can carry
   same-shape SDK events from both implementations.
3. **Stable cross-runtime labels across n=3.** The exact endpoint counts
   vary slightly between iterations, but classifications do not.
4. **A v0 attribution boundary.** IP-only network endpoints are
   insufficient for provider-host classification. The comparator's
   refusal to guess is a feature, not a failure.

## Threats to Validity

1. **"Same contract" is a manual judgement.** Mitigated by the
   workload-contract checker, which ran per iteration in the workflow
   before artifact upload. A contract violation fails the iteration.
2. **Provider auth probes are not the runtime's fault.** The live data
   shows why this is hard: v0 archives carry IP endpoints, not provider
   hostnames. Provider attribution is therefore deferred unless the
   evidence format gains host/DNS binding.
3. **Single-host bias.** All captures ran on the same
   `assay-bpf-runner` VM. Kernel-specific quirks are controlled across
   arms but not portable to other hosts without re-running.
4. **One snapshot in time.** SDK versions move fast. The source package
   pins are `@openai/agents@0.11.4` and `@google/genai@2.6.0`; rerun
   the experiment if either changes materially.
5. **Capability_surface v0 granularity.** `capability_surface.v0` still
   records touched paths undifferentiated. Operation-aware rows now come
   from optional `layers/kernel.ndjson` open metadata and only cover
   successful open-style events; unlink/remove and fd-level reads/writes
   remain outside this projection.
6. **The drift report is not a security claim.** "Runtime B contacts more
   IP endpoints" is a runtime-selection input, not a verdict that the
   runtime is insecure.

## What still does NOT prove

- **Provider-level network attribution.** Live network rows are IP-based.
  They do not prove provider-induced drift without a hostname/DNS layer.
- **Full filesystem semantics.** The new kernel row can classify
  open-style `read`/`write`/`create`/`truncate`/`append` observations
  when `layers/kernel.ndjson` carries open metadata. It still does not
  claim unlink/remove, fd-level read/write byte counts, or complete
  per-path access counts.
- **Normalized task-path equivalence.** The comparator treats absolute
  run-local paths as evidence. It does not yet normalize
  `<arm-run>/workdir/fixture-input.txt` to a shared logical
  `fixture-input` alias for the operation-aware row.
- **Cross-distro portability.** All captures are Linux/kernel-specific.
- **N > 3 stability.** n=3 is enough for this shape claim; if future runs
  show label flips, bump to n>=5.
- **Latency, token-cost, or model-output quality.** Deliberately out of
  scope.

## Reproduction commands

Stdlib-only on the comparator side. Live workload capture requires the
workflow secrets and the delegated Linux/eBPF runner.

```bash
export REPO_ROOT="$(git rev-parse --show-toplevel)"

# 1. Synthetic-fixture smoke run (no API keys, no Runner host).
python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/drift.py" \
  --archive-a "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-a-openai" \
  --archive-b "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/fixtures/arm-b-gemini" \
  --fixture-path /tmp/work/fixture-input.txt \
  --fixture-path /tmp/work/fixture-output.txt \
  --out-md /tmp/drift-smoke.md

# 2. Unit tests.
python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare" \
  -p 'test_*.py'

python3 -m unittest discover \
  -s "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/contract-checker" \
  -p 'test_*.py'

# 3. Verify committed live archive health.
for archive in \
  "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/a0/*/archive.tar.gz \
  "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/b0/*/archive.tar.gz
do
  python3 "$REPO_ROOT/docs/experiments/cross-runtime-drift-2026-05/compare/health_gate.py" \
    --archive "$archive"
done

# 4. Inspect the live drift reports.
for report in "$REPO_ROOT"/docs/experiments/cross-runtime-drift-2026-05/runs/drift/drift_pair_*.md
do
  printf '\n%s\n' "$report"
  sed -n '1,120p' "$report"
done
```

## Pinned versions for this live baseline

- Assay head: `e3f6ef9d`
- Workflow run: [26398427430](https://github.com/Rul1an/assay/actions/runs/26398427430)
- Runner boundary: `assay runner-spike`, Linux/eBPF + cgroup v2 on
  `assay-bpf-runner`
- OpenAI workload package pin: `@openai/agents@0.11.4`
- Gemini workload package pin: `@google/genai@2.6.0`
- OpenAI model: `gpt-4o-mini`
- Gemini model: `gemini-2.5-flash`
- Node: workflow preflight requires Node 22+
- Comparator/checker: Python stdlib only
