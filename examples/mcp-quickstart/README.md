# MCP Quickstart

Run one local, bounded MCP tool call through Assay in under 2 minutes.

## What this does

Wraps the included local MCP mock with Assay policy enforcement. The mock performs
no network request and no external effect. The runner retains the raw MCP streams
and one `assay.tool.decision` record under `.assay/quickstart/`.

## Prerequisites

- Assay CLI via the release installer: `curl -fsSL https://getassay.dev/install.sh | sh`
- Python 3

Source-build alternative (requires Rust):

```bash
cargo install assay-cli --version 7.0.3 --locked
```

## Run it

```bash
python3 examples/mcp-quickstart/run.py
```

For v7.0.3, run this from a source checkout or an extracted published CLI archive.
The installer is binary-only and does not carry this bounded quickstart.
It verifies the archive against its published SHA-256 sidecar before extraction.
Signed-manifest verification runs when `cosign` is available; GitHub provenance
verification is opt-in with `ASSAY_REQUIRE_PROVENANCE=1`.
See the [installation guidance](../../README.md#quickstart) for verification limits.

Captured runner output:

```text
assay quickstart: PASS
mcp_requests=initialize,tools/list,tools/call
decision=allow tool=read_file
decision_artifact=.assay/quickstart/decisions.ndjson
non_claim=forwarded_to_local_mock_only
```

The summary is emitted only after `initialize`, `tools/list`, and `tools/call`
all return, Assay exits cleanly on EOF, and the decision record is readable. A
timeout, missing child, unreadable record, or non-zero exit fails the runner.

## What's in the policy

The `read_file` and `list_dir` schemas accept literal POSIX paths beneath
`/tmp/assay-demo/`. They reject dot and parent components, doubled separators,
backslashes and ASCII control characters. The MCP server must enforce its own
filesystem boundary when resolving paths, including symbolic links.

See the [quickstart policy](policy.yaml) for the complete schemas.

## Next steps

- **Export evidence**: `assay evidence export --profile profile.yaml --out evidence.tar.gz`
- **Add to CI**: copy the [GitHub Action snippet](../../README.md#gate-your-ci) to your workflow
- **Generate policy from behavior**: `assay policy generate --from-trace trace.jsonl`
