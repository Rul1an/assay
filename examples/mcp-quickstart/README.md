# MCP Quickstart

Run one local, bounded MCP tool call through Assay in under 2 minutes.

## What this does

Wraps the included local MCP mock with Assay policy enforcement. The mock performs
no network request and no external effect. The runner retains the raw MCP streams
and one `assay.tool.decision` record under `.assay/quickstart/`.

## Prerequisites

- Assay CLI via the verified release installer: `curl -fsSL https://getassay.dev/install.sh | sh`
- Python 3

Source-build alternative (requires Rust):

```bash
cargo install assay-cli --version 5.4.0 --locked
```

## Run it

```bash
python3 examples/mcp-quickstart/run.py
```

Run this from a source checkout or the root of an extracted CLI release archive.
The installer installs the binary; the archive carries this bounded quickstart.

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

```yaml
# policy.yaml - minimal MCP guardrail
version: "2.0"
name: "mcp-quickstart"

tools:
  allow:
    - "read_file"
    - "list_dir"
  deny:
    - "exec"
    - "shell"
    - "write_file"

schemas:
  read_file:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/tmp/assay-demo/.*"
        minLength: 1
    required: ["path"]

  list_dir:
    type: object
    additionalProperties: false
    properties:
      path:
        type: string
        pattern: "^/tmp/assay-demo/.*"
        minLength: 1
    required: ["path"]
```

## Next steps

- **Export evidence**: `assay evidence export --profile profile.yaml --out evidence.tar.gz`
- **Add to CI**: copy the [GitHub Action snippet](../../README.md#gate-your-ci) to your workflow
- **Generate policy from behavior**: `assay policy generate --from-trace trace.jsonl`
