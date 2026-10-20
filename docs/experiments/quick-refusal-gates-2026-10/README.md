# Three Quick-tier refusal gates — 2026-10-20

This compact archive retains a Linux confirmation of three existing regression
controls: duplicate archive members, decoded-byte limits and gzip-trailer
validation. No product or test behavior changes. Leaves
[#2174](https://github.com/Rul1an/assay/issues/2174) open for the remaining gate
matrix, judge contracts and subprocess bounds.

## Observed result

Measured source: `05fb46761b6f648ff458d500b155aefb81fb1ee0`.
Host: Ubuntu 24.04, Linux aarch64, kernel 6.8.0-147, rustc 1.96.0.
The original workspace and complete compiler identity are in
`retained/results/metadata.json`. Cargo used one job, incremental compilation off,
and dev/test debug information off; this is not a release-binary measurement.

| Case | Quick-suite observation | Existing specific test |
|---|---|---|
| Unchanged baseline | Passed twice | All three passed |
| Duplicate refusal disabled (S4) | One duplicate bypass, assertion failed | Failed: Passed versus Blocked |
| Decoded-byte ceiling removed (E2) | One decode-bomb bypass, assertion failed | Failed: Passed versus Blocked |
| Trailer drain stubbed (E5) | CRLF invariant failed first | CRC message assertion failed |
| Wrong duplicate error code, refusal retained | Refusing-code assertion failed | Not separately run |
| Duplicate message wording only | Passed | Not separately run |
| Original bytes restored | Passed | All three passed |

All 17 exact-filter invocations executed one test: ten expected successes and
seven expected assertion failures. There were no compile failures, skipped tests,
timeouts or resource stops. Each mutation/control rebuilt `assay-evidence`;
its paired specific test used the same identified binary. Restored source hashes
matched all 5,543 regular files in the source archive. The restored test binary
hash equalled the baseline hash.

For E5, the whole-suite failure does **not** prove its later CRC message assertion
ran: the earlier CRLF invariant stops it. The separate CRC-specific test fails
because the refusal says `Data after the end of the gzip stream` instead of
`Gzip trailer`. Read the raw logs for that distinction.

[PR #2945 already recorded these repaired controls](https://github.com/Rul1an/assay/pull/2945#issuecomment-5644692596).
The selected simulation sources are byte-identical to that merged repair, while
the verifier changed; `historical-source-comparison.json` records the Git blobs.
This run supplies current-source Linux logs, not a newly discovered defect.

Claude and Argon advised; Codex/root operated the probe. The retained independent
review by `codex/3103_final_review` inspected source and evidence without rerunning
the tests. Its original scope was the larger local packet, not this publication
commit. Publisher identity does not authenticate reviewer identity or independence.
`PUBLICATION.json` names the omitted material; retained files are unchanged.

## Inspect and replay

Use a disposable Linux aarch64 environment with Python 3, Git, Rust 1.96.0 and the
normal workspace build prerequisites. The recorded VM had approximately 3 GiB
RAM and 5.7 GiB free disk before the run; these are observations, not minimum
requirements. Cargo may download locked dependencies. The original runner has
per-command time/output/resource ceilings, uses one compiler job and restores
verifier bytes on failure. Its stop paths are not qualified as a general runner.

Copy this directory into an empty working directory and verify before extraction:

```sh
sha256sum -c quick-refusals-2026-10-20.sha256 &&
  test ! -e retained && tar -xzf quick-refusals-2026-10-20.tar.gz
python3 - <<'PY'
import hashlib, json
from pathlib import Path
root = Path('retained')
for name, expected in json.loads((root / 'PUBLICATION-MANIFEST.json').read_text()).items():
    if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
        raise SystemExit('Hash mismatch: ' + name)
print('Retained inventory matches')
PY
```

Obtain the pinned source from an Assay Git clone containing the commit, then
prepare a fresh replay directory (do not reuse or overwrite the historical run):

```sh
mkdir replay && mkdir replay/source &&
  git -C /absolute/path/to/assay archive 05fb46761b6f648ff458d500b155aefb81fb1ee0 |
  tar -xf - -C replay/source
```

Verify source bytes and adapt only the original runner's workspace path:

```sh
python3 - <<'PY'
import hashlib, json
from pathlib import Path
root = Path('replay/source').resolve()
expected = json.loads(Path('retained/results/source-files.json').read_text())
actual = {str(p.relative_to(root)): hashlib.sha256(p.read_bytes()).hexdigest()
          for p in root.rglob('*') if p.is_file()}
if actual != expected:
    raise SystemExit('Source inventory mismatch')
runner = Path('retained/probe.py').read_text()
old = "ROOT=pathlib.Path('/home/ubuntu/2174-probe/source')"
if runner.count(old) != 1:
    raise SystemExit('Unexpected historical runner')
runner = runner.replace(old, 'ROOT=pathlib.Path(' + repr(str(root)) + ')')
Path('replay/probe.py').write_text(runner)
print('Source verified; only runner workspace path adapted')
PY
python3 replay/probe.py
```

The runner writes new logs, patches, source/binary hashes and resource samples to
`replay/results/`; it refuses an existing results directory. Compare each named
outcome and failure assertion with the table and retained logs. A nonzero test
exit alone is not a mutation kill: compilation errors or empty filters are not
valid evidence. New binary hashes may differ with the build path/toolchain;
within each run the restored binary must match its own baseline. Preserve fresh
results separately rather than replacing the historical files.

## Claim boundary

No missing assertion was demonstrated for these three mutations, so no repair was
made. This is a focused subset of tests reached by the workspace CI command, not
a full workspace run or hosted CI result. It establishes no complete verifier
coverage, suite-wide mutation score, real-world detection rate, independent
runtime reproduction, or completion of #2174.
