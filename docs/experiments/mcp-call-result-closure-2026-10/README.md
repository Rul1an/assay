# Selected MCP call-result closure: a small offline reproducer

This experiment asks whether explicitly selected MCP requests have terminal
results in a retained trace. It is a source-based reproducer, not a new Assay CLI
feature. It follows the observable-call suggestion in
[W3C message 0092](https://lists.w3.org/Archives/Public/public-agent-conformance/2026Oct/0092.html)
and supplies inspectable material for our [0093 report](https://lists.w3.org/Archives/Public/public-agent-conformance/2026Oct/0093.html).
It uses original synthetic fixtures; it does not import or repeat Nicolás Rocchia's
logs, model calls or judge experiment.

## What it checks

`adapter.py` joins requests/results by typed ID (number differs from string) and
attempt, under an explicit selection, source digest and inclusive observation-line
cutoff. It retains native line bytes and classifier conclusions alongside the
projection into `closure_check.py`'s [normalized contract](CONTRACT.md).

| Native evidence | Adapter interpretation |
| --- | --- |
| Valid terminal result for each selected attempt by cutoff | `supported`, under accepted source/coverage premises |
| Terminal tool-level failure (`isError`) | Terminal, not business success; native failure remains visible |
| No terminal result, protocol error or input-required | `not_established`; no physical-pending inference |
| Invalid evidence, missing or ambiguous selection, wrong digest | `no_claim`, with a refusal reason |
| Later result or different attempt/typed ID | Cannot close the selected attempt at the cutoff |

Support is **Assay-declared MCP call-result terminality** only. It is not evidence
of physical effects, business success, hidden-call completeness or general absence
of open work. Session, source acceptance, continuity and coverage are caller
premises, not authenticated observations. Native JSON-RPC does not establish a
physical pending state: the adapter cannot return `contradicted`, although the
normalized checker can do so for a separately accepted pending observation.

## Inspect without compiling

From this directory, `python3 independent_reproducer.py record/result.json`
recounts the 21 stored outcomes against the supplied synthetic bytes. It does not
execute Assay or CA and is not a second person's independent implementation.
`shasum -a 256 -c SHA256SUMS` checks the published package bytes, not their truth.

## Reproduce (macOS, Python 3.11+, Rust and Git)

Use a fresh checkout of this package revision and a separate CA checkout. The
Assay source pin is in `build_parser.py`; the CA pin is in `run_probe.py`. Those
constants drive both setup below and execution. The repositories are public:

```sh
git clone https://github.com/Rul1an/assay.git
cd assay
# Check out the immutable package commit linked with this example.
probe="$PWD/docs/experiments/mcp-call-result-closure-2026-10"
git clone https://github.com/corpus-adequacy/corpus-adequacy.git /tmp/closure-ca
ca_pin=$(PYTHONPATH="$probe" python3 -c 'from run_probe import CA_PIN; print(CA_PIN)')
git -C /tmp/closure-ca checkout --detach "$ca_pin"
```

The locked Cargo dependencies must already be cached. For first-time setup only,
use a fresh build directory to materialize the capsule, then fetch its locked
dependencies as described below. Network setup is separate from the offline run.
After setup, one command runs the native cases, checker tests, actual adapter
mutations and finite-cohort recount, with new build and output directories:

```sh
TMPDIR=/tmp "$probe/run.sh" "$PWD" /tmp/closure-ca \
  /tmp/closure-build /tmp/closure-result
```

`build_parser.py` currently creates the capsule then builds with `--offline`.
On an uncached machine that build fails with Cargo's missing-package error.
Fetch once with `cargo fetch --locked --manifest-path /tmp/closure-build/Cargo.toml`,
then rerun the command with a different fresh build directory, such as
`/tmp/closure-build-cached`. Do not reuse an existing result directory. This
explicit setup failure is not a measurement failure or a passing run.

Admission requires at least 5 GB free disk and 5% free configured swap, and no
other Cargo/rustc/clippy compile. The checks are macOS-specific; Linux/Windows
execution is not claimed. These are admission checks, not continuous enforcement
or a hostile-input sandbox. Use only the fixed trusted synthetic fixtures.
The wrapper accepts at most 64 KiB, depth 32; parser child timeout 5s, each CA
vector 8s, whole CA invocation 120s, capsule build 180s. Rust dependencies are
locked in `Cargo.lock`; Python code uses the standard library.

## Results and how to read them

The [record](record/) contains the public-package rerun, separate from the earlier
private run reported in 0093. Compare case IDs, fixture digests and native
`(outcome, reason)`, not whole result-file hashes: build paths, binary identity,
resource snapshots and toolchain metadata can differ across machines.

- 21 fixed native cases; fixture expectations are visible and same-author.
- 3 adapter test methods and 29 normalized-checker test methods.
- 7 declared changes to the actual Python adapter: protocol-error terminality,
  input-required terminality, typed ID, attempt, cutoff, source binding and reason.
- A positive control and an inert control. The inert limit change is inert only
  for these small fixtures, not for arbitrary inputs near the size boundary.
- A separately written recount from native bytes, by the same author. It imports
  no parser/checker code but is not a second person's execution or a general MCP
  implementation. `independent_reproducer.py` means a separate code path only.

The CA report's percentage applies only to those seven declared changes. No Rust
classifier mutations, live tool/model run, generic protocol adequacy or installed
Assay behavior is measured. The source capsule reuses six unmodified Assay modules,
with its own recorded dependency features; full Assay build equivalence is not
asserted. Derived observation blobs bind the projected representation, not an
independent execution witness. Whole-input validation may refuse malformed data
even after the selection cutoff.

See [PROVENANCE.md](PROVENANCE.md) for reuse, license and export changes.
