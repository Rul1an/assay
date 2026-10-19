# Bounded bundle-verifier refusals — 2026-10-19

This compact package preserves eight measured inputs, ten raw process results,
the original runner and an independent retained-result review. No production
behavior changes. Leaves [#2494](https://github.com/Rul1an/assay/issues/2494) open.

## Observed result

The published Assay 6.14.1 Linux aarch64 GNU binary, source
`098ebbeb2efe19c3b8fdc6bc41250c80169cc06c`, was executed on an existing Ubuntu
24.04.5 VM. Its SHA-256 is recorded in `retained/results/host.json` and checked by
the retained runner. Selected verifier, limits and CLI sources were byte-identical
to main `89abccde9b0e3fa7219ecd746d025ca2b018f346`; see the retained report and
source-parity record. This is selected-source comparison, not source-to-binary proof.

| Input | Routes | Observed result |
|---|---|---|
| Original; compliant repack | original file + stdin; repack file | Three successful verifications |
| Equal-length event-0 content change | file + stdin | `IntegrityEventHash`, seq 0 |
| Expected events hash changed in manifest | file | `IntegrityManifestHash` |
| Events member missing | file | `ContractMissingFile` |
| One-bit gzip CRC corruption | file | `IntegrityGzip`, trailer |
| Declared manifest size 10 MiB + 1, absent body | file | `LimitFileSize` |
| Declared manifest size exactly 10 MiB, absent body | file | `ContractInvalidJson`, EOF |

All ten predeclared expectations matched: three positive invocations and seven
expected refusals, no retries or timeouts. All stdout was empty. Read individual
JSON/stdout/stderr records, not just `complete.json`. Diagnostic codes and strings
are oracles for this pinned binary, not a newly frozen CLI contract. The retained
runner checks the typed code for most refusals; the historical result review also
checked the more specific trailer, missing-file and EOF diagnostics.

Argon advised the case matrix; Codex/root constructed inputs and operated the VM;
`codex/2497_final_review` independently inspected the retained results without
running the binary. Reviewer identity and independence are declared, not externally
authenticated. This was not a fresh-host or independently operated recipient run.

## Replay the original inputs

Use a fresh empty working directory on Linux aarch64 compatible with the GNU
release, with Python 3 and approximately 100 MiB available for the executable and
small inputs/outputs. This is a practical allowance, not a measured minimum.
No Rust build is needed. Copy this directory there, then:

```sh
sha256sum -c refusal-2026-10-19.sha256 &&
  test ! -e retained && tar -xzf refusal-2026-10-19.tar.gz
```

Verify the publication inventory before proceeding:

```sh
python3 - <<'PY'
import hashlib, json
from pathlib import Path
root = Path('retained')
for name, expected in json.loads((root / 'PUBLICATION-MANIFEST.json').read_text()).items():
    if hashlib.sha256((root / name).read_bytes()).hexdigest() != expected:
        raise SystemExit('Hash mismatch: ' + name)
print('Publication inventory matches')
PY
```

Obtain the executable from the
[6.14.1 aarch64 GNU release archive](https://github.com/Rul1an/assay/releases/download/v6.14.1/assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz).
Optional fresh provenance check before extraction:
`gh attestation verify assay-v6.14.1-aarch64-unknown-linux-gnu.tar.gz --repo Rul1an/assay`.
The preparation below verifies the extracted executable against the historical
binary hash before copying or running it. A matching hash identifies bytes; it
is not fresh attestation verification.

Pass the absolute path to that extracted executable:

```sh
python3 - /absolute/path/to/assay <<'PY'
import hashlib, json, shutil, sys
from pathlib import Path
base = Path('retained')
binary = Path(sys.argv[1]).resolve()
expected = json.loads((base / 'results/host.json').read_text())['binary_sha256']
if hashlib.sha256(binary.read_bytes()).hexdigest() != expected:
    raise SystemExit('Binary hash mismatch')
runner = (base / 'run.py').read_text()
old = "binary=pathlib.Path('/home/ubuntu/package/replay/release/assay-v6.14.1-aarch64-unknown-linux-gnu/assay')"
if runner.count(old) != 1:
    raise SystemExit('Unexpected historical runner')
out = Path('replay')
out.mkdir(exist_ok=False)
shutil.copytree(base / 'inputs', out / 'inputs')
shutil.copy2(base / 'cases.json', out / 'cases.json')
shutil.copy2(binary, out / 'assay')
(out / 'assay').chmod(0o755)
(out / 'run.py').write_text(runner.replace(old, "binary=root/'assay'"))
print('Prepared replay; historical runner and results remain unchanged')
PY
python3 replay/run.py > replay/execution.stdout 2> replay/execution.stderr
replay_exit=$?
printf '%s\n' "$replay_exit" > replay/execution.exit
```

Do not use Python `-O`: the historical runner uses assertions for byte guards.
Preparation refuses an existing `replay/`; execution refuses an existing results
directory. Preserve a failed attempt and use a new working directory for another
attempt. The runner bounds each process to 60 seconds and each output file to
64 KiB, retains all ten outcomes and returns nonzero on an oracle mismatch.
Inspect the saved exit, `replay/results/complete.json` and every process record.
An exit of zero should accompany ten matches; no result is clean merely because
its output is absent. New argv paths, timestamps and runner hash will differ.

Only the fixed binary path is changed in the replay copy. The input hashes,
commands, bounds and expectation logic are unchanged. The original measured
`retained/run.py` remains byte-preserved; `host.json` in each new run records the
adapted runner hash. Unexecuted position/prefix drafts are retained for provenance
but are not in the executed `cases.json` matrix. Reconstructing gzip files on a
new Python version may change archive bytes; replay the retained inputs for exact
byte identity. Construction scripts are historical aids, not required for replay.

## Scope and retention

The archive is below the 4 MiB publication ceiling and includes original bytes,
raw outputs, source snapshots, construction decisions and the historical review.
`PUBLICATION-MANIFEST.json` inventories this publication. `RESULT-MANIFEST.json`
is the original larger inventory: its raw Argon design transcript is deliberately
not included. `RETENTION.md` explains that omission and the historical review's
scope. No release binary or agent transcript is bundled.

These selected cases demonstrate unsigned internal consistency checks. They do
not establish authenticity, resistance to a fully recomputed bundle, provider
outcomes, detection accuracy, false-positive/negative rates or exhaustive coverage.
The size pair demonstrates declared-member preflight and its strict boundary,
not measured RSS protection or valid-input acceptance at exactly 10 MiB. Check
attribution uses source ordering and diagnostics, not an instrumented trace.

Remaining #2494 scope includes broader workloads, signed-attestation cases,
resource measurements and justified quality denominators. This package supplies
replayable evidence for the selected cases, not completion of that scope.

The separate [events-byte resource pilot](../verifier-events-resource-2026-10/README.md)
retains complete 50/250/500 MiB inputs and a 500 MiB + 1-byte refusal, with
measured process RSS and replay instructions. Its synthetic workload and header
preflight do not establish a universal resource bound.
