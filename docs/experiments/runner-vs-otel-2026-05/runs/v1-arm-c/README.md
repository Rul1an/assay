# v1 Arm C baseline (delegated, real Linux/eBPF)

Three dual-capture iterations on the `assay-bpf-runner` self-hosted runner.
This directory is committed evidence; the GitHub Actions workflow run
that produced it is the canonical source.

## Provenance

| Field | Value |
|---|---|
| Workflow | [`.github/workflows/runner-otel-experiment.yml`](../../../../../.github/workflows/runner-otel-experiment.yml) |
| Workflow run | <https://github.com/Rul1an/assay/actions/runs/26372344619> |
| Head commit | `c6508780c8f77b38c773b7622fc4047fa481f826` |
| Date | 2026-05-24 |
| Dispatch inputs | `repetitions=3`, `require_binding_match=true`, `build_ebpf=true` |
| Runner kernel | `6.8.0-117-generic` (Ubuntu 24.04.3 LTS, ARM64) |
| Runner labels | `[self-hosted, linux, assay-bpf-runner]` |
| Rust | `cargo 1.94.0`, `rustc 1.94.0` |
| Node | `v22.16.0` |
| Comparator | `compare.py` with `--require-binding-match` (exit code 0 for all three) |

## Per-run layout

Each `run_arm_c_<timestamp>_<i>/` directory contains:

| File | Source |
|---|---|
| `trace.json` | OTLP/JSON exported by `workload/src/otel-setup.ts` and post-decorated by `compare/bind-archive.py` with the `assay.archive.created` event. |
| `matrix.json` | `compare.py` output: the canonical machine-readable field matrix. |
| `matrix.md` | `compare.py` output: human-readable matrix. |
| `archive-contents/` | Extracted from the run's `assay.runner.archive_manifest.v0` `.tar.gz`. The raw tarball itself is **not** tracked; reviewers can re-tar locally or fetch from the workflow artifact. |

## What is and is not tracked here

Tracked (small, schema-conforming, useful for reviewers):

- `matrix.json` and `matrix.md` per run
- `archive-contents/manifest.json` (run identity + per-file digests)
- `archive-contents/capability-surface.json` (filesystem paths,
  network endpoints, process execs, mcp tools, policy decisions)
- `archive-contents/observation-health.json` (kernel layer, ringbuf
  drops, cgroup correlation, policy/sdk layer status)
- `archive-contents/correlation-report.json` (status + bindings)
- `archive-contents/events.ndjson` and `archive-contents/layers/*.ndjson`
  (raw event streams; small for the v1 deterministic workload)
- `trace.json` (small OTLP/JSON export)

Not tracked here:

- The raw `archive.tar.gz` tarball. Removed deliberately. For larger
  Arm C workloads in the future (longer runs, more events, more files),
  the entire `archive-contents/` directory will exceed reasonable git
  payload too; the policy then is to keep only `matrix.{json,md}` here
  and link to the GitHub Actions artifact URL above for full
  archives.
- `workdir/` from the workload (scratch directory contents,
  re-generated on every run).

## Sanity check

```bash
# Recompute the digest of run 1's manifest and confirm it matches the
# digest the trace's assay.archive.created event recorded.
python3 -c "
import hashlib
b = open('runs/v1-arm-c/run_arm_c_20260524T205016Z_1/archive-contents/manifest.json', 'rb').read()
print('manifest digest:', 'sha256:' + hashlib.sha256(b).hexdigest())
"

# Compare to:
python3 -c "
import json
m = json.load(open('runs/v1-arm-c/run_arm_c_20260524T205016Z_1/matrix.json'))
print('trace-side digest:', m['trace_observation']['manifest_digest'])
print('archive-side digest:', m['runner_observation']['manifest_digest'])
print('binding:', m['summary']['manifest_digest_binding'])
"
```

Both digests must match each other and the recomputed value. If they
do not, the `compare/bind-archive.py` post-step or the extraction
above is wrong; the published evidence claim depends on this.
