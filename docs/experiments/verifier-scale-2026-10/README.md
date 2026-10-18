# Assay verifier baseline and same-day repeatability package

This repository baseline retains the original baseline, the corrected-control run, a later second window, raw measurements, generator source and provenance. See retained/RETENTION.md for the compact publication inventory. It is not a published release, CI threshold, or complete resolution of #2494.

## Reproduce the measurement

Requires Linux aarch64 compatible with the released GNU binary (measured on Ubuntu 24.04), Python 3, GNU `/usr/bin/time`, and at least 2 GiB free disk. Download the pinned release archive once; subsequent measurement needs no build or network access. Use a new output directory for every attempt; keep failures and do not overwrite results. Do not run Python with `-O`: the original pinned script uses assertions for its guards.

1. Run `sha256sum -c baseline-2026-10-18.sha256`, then `tar -xzf baseline-2026-10-18.tar.gz` into this directory only if `retained/` does not already exist. Verify `retained/MANIFEST.json` using the snippet below. Hashes detect changes; they do not authenticate a sender.
2. Download [assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz](https://github.com/Rul1an/assay/releases/download/v6.14.1/assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz). The preparation helper verifies its pinned SHA-256 before extraction.
3. Run `python3 prepare_replay.py /absolute/existing-parent/new-replay /absolute/path/to/assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz`.
4. Run `python3 /absolute/existing-parent/new-replay/run_linux_v2.py` on Linux aarch64. Record stdout, stderr and exit status, including failures. The script verifies hashes before running and refuses an existing results directory.
5. Inspect `results/complete.json`, every run's JSON/stdout/stderr/GNU-time file, and `results/host.json`. There should be three expected content-hash refusals, three successful warmups and fifteen successful measured verifications. A completion file alone is insufficient proof.

```python
import hashlib, json
from pathlib import Path
root = Path('retained')
for name, expected in json.loads((root / 'MANIFEST.json').read_text()).items():
    h = hashlib.sha256()
    with (root / name).open('rb') as f:
        for chunk in iter(lambda: f.read(1048576), b''):
            h.update(chunk)
    if h.hexdigest() != expected:
        raise SystemExit('Hash mismatch: ' + name)
print('All manifest entries match')
```

The replay preparation helper only copies hash-checked fixtures and extracts the single pinned executable. It does not change the original measurement script. The original inputs map, release pin and attestation output are under `retained/`. Retained verification output is not a fresh online attestation check. Optional live validation: `gh attestation verify /absolute/path/to/assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz --repo Rul1an/assay`.

## Regenerate fixtures from source (optional)

Exact replay uses the retained original bytes. To regenerate, check out Assay commit `63f3ee6f5d8b3eb1d48e91912598d64aaff3eeb2` in a separate clean worktree, copy `retained/verifier_scale_generate.rs` to `crates/assay-evidence/examples/verifier_scale_generate.rs` (create the examples directory), then run:

```sh
CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo build -p assay-evidence --example verifier_scale_generate
mkdir regenerated
ASSAY_PERF_WORKLOAD=small,typical-pr,large target/debug/examples/verifier_scale_generate regenerated
```

The recorded Rust version is in `retained/toolchain.txt`; the pinned repository checkout supplies library source. Before substituting regenerated fixtures, compare their hashes with `retained/inputs-v2.json`. A mismatch is a new input series, not an exact replay. Generation is outside measured processes.

## Reading the results

See `retained/COMPARISON.md`, `retained/comparison.json` and the historical independent review. Both event count and payload bytes change between profiles. Time includes CLI startup and the GNU-time wrapper; peak RSS covers the CLI, not fixture generation or the Python orchestrator. Warm filesystem workflow, fresh CLI processes, shared VM; no cold-cache, pure count-scaling, hostile-input worst-case or population-confidence claim. Repacked corrupted gzip bytes can differ by creation time; decoded mutation, unchanged manifest and original content hashes are checked separately.

The original failed control (allow -> deny shortened the stream) remains under retained/initial-failed-control. All successful measurement windows use the corrected equal-length allow -> block control. No performance result was discarded or replaced. The second window is a separate same-day repetition, not independent hardware replication.

## Results across two time windows

Same released Assay 6.14.1 binary, SHA-256 `f97312764e2b63494a4c13e6ba2341ce1405019c97ac63a78edd08d5ab92b703`, source `098ebbeb2efe19c3b8fdc6bc41250c80169cc06c`. Same fixture bytes and unchanged run_linux_v2.py. Generator base `63f3ee6f5d8b3eb1d48e91912598d64aaff3eeb2` plus retained adapter; original worktree recorded in retained/provenance.json.

First measured process starts (UTC): 2026-10-18T21:13:33.210664+00:00 and 2026-10-18T21:21:37.730624+00:00; gap 8.08 minutes. Each window: 5 measured fresh processes per profile after one warmup, rotating order. Same shared Linux ARM64 VM, no cache flush.

| Profile | Window 1 median ms (range) | Window 2 median ms (range) | Median peak RSS MiB W1 / W2 |
|---|---:|---:|---:|
| small | 36.64 (32.07–36.91) | 37.78 (35.24–101.28) | 12.06 / 12.06 |
| typical-pr | 257.06 (192.36–314.01) | 374.08 (210.17–586.45) | 15.88 / 15.88 |
| large | 2503.01 (2147.16–3956.58) | 2755.31 (2455.62–5042.97) | 84.75 / 84.75 |

All 30 measured positive verifications and six warmups across the two windows exited 0. All six corrected corrupted controls were rejected with Content hash mismatch at seq 0. The failed initial size-changing control remains retained separately. These are descriptive observations: a later median difference is neither a regression nor an improvement because the binary and inputs are unchanged and host conditions are not isolated. Do not pool these samples into an independent-population confidence interval or set a CI threshold from them.

Time includes process startup and GNU time wrapper; memory is the CLI high-water RSS. Both event counts and payload sizes increase between profiles, so this is not a pure event-count slope. The second window is same-day repeatability, not long-term stability or independent-host replication. Replay instructions are above. No production changes, merge, release, or issue closure are claimed.

Leaves [#2494](https://github.com/Rul1an/assay/issues/2494) open for broader resource and detection-quality measurements, longer-term repeatability and defensible CI budgets.
