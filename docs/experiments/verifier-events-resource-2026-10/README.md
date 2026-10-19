# Events-byte resource pilot and replay — 2026-10-19

This compact package retains four complete inputs, thirteen raw process results,
the original generator and runner, provenance and an independent retained-result
review. Leaves [#2494](https://github.com/Rul1an/assay/issues/2494) open.

## Observed result

The measured executable is the published Assay **v6.14.1 Linux aarch64 GNU** binary,
SHA-256 `f97312764e2b63494a4c13e6ba2341ce1405019c97ac63a78edd08d5ab92b703`,
release source `098ebbeb2efe19c3b8fdc6bc41250c80169cc06c`. Five selected source
files were byte-identical to main `20b9aa018d4a0d628698680871f1824a0b44aea3`;
this is selected-source comparison, not source-to-binary or whole-tree proof.

One seed control preceded three rotated rounds of the four cases on an existing
shared Ubuntu Linux ARM64 VM. All **13 expected outcomes matched**: ten successes
including the seed, and three expected refusals. No retries, timeouts or signals
were recorded. All stdout was empty. Raw records, stderr and GNU-time files are
retained; a completion file alone is insufficient evidence.

| `events.ndjson` bytes | Runs | Peak RSS range (MiB) | Median GNU elapsed (s) | Outcome |
|---|---:|---:|---:|---|
| 50 MiB | 3 | 12.125–12.250 | 0.08 | exit 0 |
| 250 MiB | 3 | 12.625 | 0.42 | exit 0 |
| 500 MiB exactly | 3 | 13.125 | 0.85 | exit 0 |
| 500 MiB + 1 byte | 3 | 12.250 | 0.00, as displayed | exit 2, `LimitFileSize` |

GNU time displays elapsed seconds with 0.01-second resolution: `0.00` does not
mean zero time. The separately recorded wrapper wall time includes polling and
is not interchangeable with GNU time. The external virtual-address limit was
1536 MiB; the initial 768 MiB proposal was amended before execution. A preliminary
missing-binary-location failure is preserved separately, not hidden as a retry.

Inputs retain original event JSON with trailing ASCII spaces before each newline.
Fixed 512 KiB lines and prefixes of 100, 500 and 1,000 events produce the three
sizes. Manifest byte hashes/counts and prefix run roots were recomputed. The
oversized input has one additional space and a **complete body**, not just an
oversized header. Compressed inputs range from 96,876 to 965,759 bytes. Event
count and compressed size grow alongside decoded size; this is not a pure causal
measurement of decoded-byte scaling.

The exact-boundary successes exercise processing of all 500 MiB. The oversized
case exercises the **declared-size preflight** before events processing, based
on source ordering and diagnostics, not an instrumented trace. It does not prove
in-stream limit enforcement or rule out decoder read-ahead. No defect or need to
change a parser/default limit was demonstrated by this workload.

## Replay on Linux

Use Linux aarch64 compatible with the GNU release, Python 3, GNU `/usr/bin/time`
and `pgrep` (procps). The original runner admits a run only with at least 4 GiB
free disk and 2 GiB `MemAvailable`, and no observed `Runner.Worker`. These are
experiment admission floors, not product minimum requirements. It applies a
1536 MiB address-space limit, a 120-second per-call timeout and 64 KiB output-file
limits. Guard failure, timeout or a signal is not a successful verifier refusal.
Use a clean environment and retain any failure; the runner does not sanitize
inherited environment variables. No build or fixture regeneration is needed.

Copy this directory into a fresh workspace and verify/extract the package:

```sh
sha256sum -c events-resource-2026-10-19.sha256 &&
  test ! -e retained && tar -xzf events-resource-2026-10-19.tar.gz
```

Obtain the executable from the
[v6.14.1 aarch64 GNU release archive](https://github.com/Rul1an/assay/releases/download/v6.14.1/assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz).
Optional fresh release provenance verification:
`gh attestation verify assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz --repo Rul1an/assay`.
The historical attestation entry in the retained pin is not fresh verification.
Pass the absolute path to the extracted executable to this preparation step:

```sh
python3 - /absolute/path/to/assay <<'PY'
import hashlib, json, pathlib, shutil, sys
root = pathlib.Path('retained')
for name, expected in json.loads((root / 'PUBLICATION-MANIFEST.json').read_text()).items():
    if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
        raise SystemExit('Publication hash mismatch: ' + name)
binary = pathlib.Path(sys.argv[1]).resolve()
pin = json.loads((root / 'release-pin.json').read_text())
if hashlib.sha256(binary.read_bytes()).hexdigest() != pin['binary_sha256']:
    raise SystemExit('Release executable hash mismatch')
out = pathlib.Path('replay')
out.mkdir(exist_ok=False)
for name in ('run.py', 'cases.json', 'release-pin.json', 'seed.tar.gz'):
    shutil.copy2(root / name, out / name)
shutil.copytree(root / 'inputs', out / 'inputs')
shutil.copy2(binary, out / 'assay')
(out / 'assay').chmod(0o755)
print('Publication and binary hashes match; fresh replay prepared')
PY
python3 replay/run.py > replay/execution.stdout 2> replay/execution.stderr
replay_exit=$?
printf '%s\n' "$replay_exit" > replay/execution.exit
```

Do not run Python with `-O`: the unchanged historical runner uses assertions for
hash guards. Preparation refuses an existing `replay/` directory. Inspect
`replay/results/complete.json`, every case JSON, stdout, stderr and `.time` file,
and `host.json`. Expect the same 13 outcomes and the exact 524,288,000-byte ceiling
in the refusal diagnostic; timing/RSS need not match the historical values.
Failure stops the series without replacement retries; inspect `failure.json`
and wrapper stderr when present. Preserve the entire attempt before another run.

The unchanged `generate.py` can regenerate fixtures in a separate empty directory
containing `seed.tar.gz`. A different Python/zlib version may change compressed
bytes: use the retained inputs for exact replay, and treat changed hashes as a
new series. Generation and input hashing are outside the timed measurements.

## Evidence boundaries and retention

Codex/root built fixtures and operated the VM; Argon advised the design.
`codex/refusal_publication_review` independently inspected the original retained
bytes and recounted all outcomes. The reviewer did not execute the product or
use a second host. These identities and independence are declarations, not
externally authenticated facts. Publication review is a separate exact-head step.

The archive is below the 4 MiB publication ceiling. `PUBLICATION-MANIFEST.json`
inventories its payload. Original `RESULT-MANIFEST.json` and `PRE-RUN-MANIFEST.json`
refer to the larger historical package, including excluded executable/transcript
bytes; do not use them as this subset's completeness inventory. `RETENTION.md`
explains the exclusions. The unchanged `independent-recount.py` belongs to the
original full-package review and requires its omitted inputs; use the replay
instructions above for this publication. No binary or agent transcript is bundled.

This synthetic, highly compressible workload provides no universal RSS bound,
production-throughput claim, cold-cache measurement, independent-host replication,
detection-quality rate or CI threshold. It does not cover every parser limit,
high-entropy sources, signed attestations or other reader routes. Low measured RSS
is inconsistent with the whole 500 MiB events member being resident in these runs;
it is not a general memory-safety proof. The broader resource, supported-profile,
repeatability and quality-denominator scope of #2494 remains open.
