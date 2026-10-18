# Check a SARIF report against its source bundle

This offline example exercises the existing Assay CLI with two packaged synthetic
bundles. It checks source correlation for a report with findings, an empty report,
and a report capped to zero results. It also exercises a valid unrelated bundle
and refusal of damaged or missing input.

## Run

Prerequisites: Python 3.9 or newer (standard library only), and an Assay binary
containing the run-level `bundle_id` fix from PR #3364. A version string alone does
not establish that the binary contains the fix. An older binary should fail the
source-identity check; do not remove that check to make it pass.

```bash
python3 examples/sarif-recipient/check.py \
  --assay /absolute/path/to/assay \
  --output /tmp/assay-sarif-recipient-run-1
```

The output directory must not exist. The script can run from any working directory;
it locates its fixtures relative to itself. No network connection, credentials,
schema validator, or build is needed. For another run, use a new output directory.

Exit 0 means all example assertions passed. Exit 1 means an assertion or command
failed; inspect `receipt.json` and the retained stdout/stderr. Each CLI invocation
has a 30-second timeout. The receipt records the actual argv, exit codes when
available, binary path/hash/version, fixture hashes, script hash, host and Python
version, and hashes of the retained output files. The archive bytes are copied
unchanged into that directory. It is a local measurement record, not a signed
attestation. Keep the directory if you want to hand the result to another reviewer.

## What the recipient checks

1. `evidence show source.tar.gz --format json` verifies the source by default.
   Its `manifest.bundle_id` is the reference content identity.
2. `evidence lint source.tar.gz --format sarif --fail-on none` produces the
   zero-finding control. Adding `--pack soc2-baseline` produces one finding:
   `soc2-baseline@1.0.0:invariant.decision_record`. The verified events contain no
   `assay.tool.decision` event. The result points to a pack rule, not a missing
   event's source line; inspect `finding.stdout` alongside `source.stdout`.
3. Adding `--max-results 0` removes the result but must retain the same run-level
   identity and disclose truncation, the dropped count and the cap. Empty results
   here do not mean no findings existed. `--fail-on none` deliberately permits
   findings; its exit 0 is not a policy pass or compliance verdict.
4. The separately verified `other-valid.tar.gz` has a different identity. The
   example compares it with each report and confirms the mismatch. This comparison
   belongs to the example: the CLI does not automatically verify an arbitrary
   SARIF/bundle pair.
5. A truncated copy of the archive and a missing path must both produce exit 2
   with empty SARIF stdout.

## Fixture provenance

`fixtures/source.tar.gz` preserves the repository's synthetic CI fixture bytes
from `tests/fixtures/evidence/test-bundle.tar.gz`. It contains five synthetic
lifecycle/filesystem/network/process events; their historical producer version is
fixture metadata, not the version of the binary running this example.

`fixtures/other-valid.tar.gz` preserves a synthetic Promptfoo assertion-component
import from the #2489 recipient rehearsal. It is used only as an unrelated valid
content identity, not to prove Promptfoo execution or source-artifact availability.
`fixtures/SHA256SUMS.json` pins both original archives. These archive hashes are
distinct from the verified content `bundle_id`.

## Limits and release acceptance

SARIF is a lossy, one-directional projection. Matching its asserted `bundle_id`
to verified evidence establishes correlation, not authentication of the report or
producer, honest lint execution, complete observation, or an external outcome.
The example does not convert SARIF back into canonical evidence. It does not prove
SOC2 compliance, full SARIF schema conformance, or GitHub Code Scanning acceptance.
Schema validation, if needed, is a separate optional check with an explicitly
pinned validator and schema; it is not a prerequisite for this example.

A source-built binary run is development evidence. After a release containing
the fix, rerun the same command with the official binary and retain its release
tag, download provenance and digest alongside the receipt before marking the
released-example criterion in #2489 complete. This example does not authenticate
release provenance automatically.
