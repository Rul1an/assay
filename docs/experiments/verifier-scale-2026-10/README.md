# Verifier baseline and replay — 2026-10-18

This package retains every raw measurement from two Linux time windows and the initial failed control, with provenance, generator source and fixture hashes. Large fixture bytes and release binaries are not bundled. The original fixture bytes remain in the local evidence package; regenerating them with the pinned generator produced identical hashes for all three profiles (retained/regeneration-parity.json). Historical reviews describe their original larger packages; see retained/RETENTION.md for this publication subset.

## Reproduce the measurement

Use a fresh copy of this directory for each attempt. Fixture generation requires the pinned Assay source and Rust toolchain; dependency downloads may require network access. Measurement requires Linux aarch64 compatible with the released GNU binary (measured on Ubuntu 24.04), Python 3, GNU `/usr/bin/time`, and at least 2 GiB free disk. A separate machine may generate fixtures, as in the original experiment. Neither generation nor compilation is measured.

1. Verify `sha256sum -c baseline-2026-10-18.sha256`, then extract with `tar -xzf baseline-2026-10-18.tar.gz` only if `retained/` does not exist. Check the internal manifest using the snippet below.
2. Check out Assay commit `63f3ee6f5d8b3eb1d48e91912598d64aaff3eeb2` in a separate clean worktree. Copy `retained/verifier_scale_generate.rs` into its `crates/assay-evidence/examples/` directory (create that directory). Use the toolchain recorded in `retained/toolchain.txt` and run these commands from the source worktree:

   ```sh
   CARGO_BUILD_JOBS=1 CARGO_INCREMENTAL=0 cargo build -p assay-evidence --example verifier_scale_generate
   ASSAY_PERF_WORKLOAD=small,typical-pr,large target/debug/examples/verifier_scale_generate /absolute/path/to/this-package/retained
   ```

   Use the freshly extracted retained directory: do not overwrite previous fixtures. The pinned checkout supplies library source; the adapter reuses the benchmark generator bodies. The original parity check reused the macOS-built generator. A later clean Linux build also matched all three fixture hashes; see the bounded receipt below. This does not establish parity on every supported host. If a regenerated hash differs, stop: those bytes are a new input series, not an exact replay.
3. Download [assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz](https://github.com/Rul1an/assay/releases/download/v6.14.1/assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz) alongside this README, preserving its filename. Optional live provenance verification: `gh attestation verify assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz --repo Rul1an/assay`.
4. Run `python3 prepare_replay.py` from the package directory. It verifies fixture, script, release archive and binary hashes, then creates an exclusive `replay/` directory. It uses fixed package-local paths. Hash failures stop preparation; an existing replay directory is refused.
5. On Linux aarch64 run `python3 replay/run_linux_v2.py`, retaining stdout, stderr and exit status for every attempt. Do not use Python `-O`: the unchanged measured script uses assertions for its guards. Once prepared, measurement needs no build or network access.
6. Inspect `replay/results/complete.json`, each run's JSON/stdout/stderr/GNU-time file and host.json. Expect three content-hash refusals, three successful warmups and fifteen measured successful verifications. A completion file alone is insufficient proof. Keep failed attempts; never silently substitute retries.

Verify the checked-in package's internal files before generating the additional fixtures:

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

The original input map and release pin are in retained/inputs-v2.json and retained/release-pin.json. Retained attestation output is not fresh signature verification. The helper extracts only the pinned executable and does not modify the original measurement script. The initial allow-to-deny control shortened the event stream and stopped at a size check; its raw results are retained under initial-failed-control. Both completed windows use equal-length allow-to-block corruption. Repacked corrupted gzip timestamps can differ; decoded mutation, unchanged manifest and original content hashes are separate properties.

## Cold Ubuntu setup and Linux replay

On 2026-10-19, a newly provisioned Ubuntu 24.04.5 aarch64 VM rebuilt the generator with the pinned source and native Rust 1.96.0. All three fixture archive hashes matched the original inputs. The unchanged runner produced 15 successful measured verifications, three successful warmups and three content-hash refusals. A separate one-bit corruption of the outer publication archive was refused by `sha256sum -c` before extraction. The [compact receipt](linux-replay-2026-10-19.json) retains input hashes, toolchain/host provenance and individual process records with stdout, stderr and GNU-time output. It identifies the original local evidence archive by hash; that larger archive is not included here.

Codex/root operated the fresh VM on the same physical Mac. Argon reviewed instructions read-only after its shell probe was refused; a separate non-building agent recounted the retained results. This is a clean Linux rebuild with independent result review, not a blind independent-recipient test or different physical hardware. Reviewer identity is declared, not externally authenticated. Timing differences do not establish improvements or regressions.

For a cold Ubuntu host, the following is a setup example before step 2. Start in an empty working directory. The tested image already provided `git`, `curl` and Python 3; the package list below makes those prerequisites explicit. The additional build packages were installed in the successful run, not proven to be a minimal dependency set.

```sh
sudo apt-get update
sudo apt-get install -y --no-install-recommends \
  git curl python3 ca-certificates build-essential pkg-config libssl-dev time
git init source
cd source
git remote add origin https://github.com/Rul1an/assay.git
git fetch --depth=1 origin 63f3ee6f5d8b3eb1d48e91912598d64aaff3eeb2
git checkout --detach FETCH_HEAD
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs -o rustup-init.sh
sh rustup-init.sh -y --profile minimal --default-toolchain none
rm rustup-init.sh
. "$HOME/.cargo/env"
# rust-toolchain.toml in this pinned checkout selects the native toolchain.
rustup show active-toolchain
rustc -Vv
cargo -V
```

Continue with the adapter copy and build commands in step 2. Use the native Linux toolchain; `aarch64-apple-darwin` in the historical toolchain record describes the original host, not the Linux installation target. The setup example lets the source checkout select its pin, rather than maintaining another Rust-version literal.

The 2 GiB free-space floor applies to replay preparation and measurement **after** compilation and fixture generation. Allow additional space for Rust, dependencies and `target/` when building on the same host. This run began with 9,253,818,368 bytes available and had 6,353,162,240 bytes available after setup; these observations are not a minimum-capacity guarantee. Corrupted control archives are generated by the runner from verified positive fixtures at runtime; they are not additional pinned inputs to prepare.

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
