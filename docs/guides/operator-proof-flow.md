# Operator Proof Flow

This is the shortest end-to-end Assay walkthrough for operators who want to
see one product story instead of three separate subsystems.

The flow shows:

1. how Assay normalizes MCP input into a stable trace
2. how the shipped `owasp-agentic-control-evidence-baseline` pack interprets
   evidence from that kind of flow
3. how a consumer verifies an Assay release offline with the shipped proof kit

This guide is intentionally narrow. It does **not** claim broad OWASP coverage,
goal-hijack detection, privilege-abuse prevention, or end-to-end supply-chain certainty.

## Before You Start

- Assay CLI installed
- `jq`
- GitHub CLI with support for:
  - `gh attestation trusted-root`
  - `gh attestation download`
  - `gh attestation verify --bundle --custom-trusted-root`
  - `gh release verify`
  - `gh release verify-asset`

If you are following the commands exactly as written below, run them from a
checkout of the Assay repository so the bundled example files are present.

## Step 1: Normalize A Modern MCP Transcript

Assay's import layer accepts modern Streamable HTTP MCP transcripts directly.
The smallest useful example looks like this:

```json
{
  "transport": "streamable-http",
  "entries": [
    {
      "timestamp_ms": 1000,
      "request": {
        "jsonrpc": "2.0",
        "id": "1",
        "method": "tools/call",
        "params": {
          "name": "read_file",
          "arguments": {
            "path": "/tmp/demo.txt"
          }
        }
      }
    },
    {
      "timestamp_ms": 1001,
      "response": {
        "jsonrpc": "2.0",
        "id": "1",
        "result": {
          "ok": true
        }
      }
    },
    {
      "timestamp_ms": 1002,
      "sse": {
        "event": "message",
        "id": "evt-1",
        "data": {
          "jsonrpc": "2.0",
          "method": "notifications/progress",
          "params": {
            "status": "done"
          }
        }
      }
    }
  ]
}
```

Import it with:

```bash
mkdir -p traces
assay import --format streamable-http session.json --out-trace traces/session.jsonl
```

What this proves:

- Assay can ingest a modern MCP transport transcript and normalize it into a
  canonical trace.
- Tool-call correlation is driven by the JSON-RPC `id`.
- Transport-only SSE/control semantics do not become extra tool findings.

For deeper MCP wrapping and policy examples, continue with
[MCP Quick Start](../mcp/quickstart.md).

## Step 2: Apply The Shipped C2 Pack

The shipped `C2` pack is:

```text
owasp-agentic-control-evidence-baseline
```

It is a **control-evidence** subset pack. It does not detect goal hijack, it
does not verify privilege abuse, and it does not prove sandboxing.

### Run A Real Failing Example

This repository includes a small evidence bundle fixture that already contains
`assay.process.exec`, but does **not** contain the governance/authz fields that
`A1-002` and `A3-001` check for, typically on `assay.tool.decision`.

Run:

```bash
assay evidence lint tests/fixtures/evidence/test-bundle.tar.gz \
  --pack owasp-agentic-control-evidence-baseline \
  --format json
```

Relevant output fragment:

```json
{
  "verified": true,
  "findings": [
    {
      "rule_id": "owasp-agentic-control-evidence-baseline@1.0.0:A1-002",
      "message": "No event contains any of the required fields: /data/reason_code, /data/approval_state"
    },
    {
      "rule_id": "owasp-agentic-control-evidence-baseline@1.0.0:A3-001",
      "message": "No event contains any of the required fields: /data/principal, /data/approval_state"
    }
  ],
  "summary": {
    "total": 2,
    "warnings": 2
  }
}
```

Why only two findings appear:

- `A1-002` fails because no event in the bundle contains `reason_code` or
  `approval_state`
- `A3-001` fails because no event in the bundle contains `principal` or
  `approval_state`
- `A5-001` **passes** because the bundle already contains `assay.process.exec`

### Minimal Passing Shape

A minimal passing bundle needs evidence equivalent to the abbreviated events
below:

```json
{"type":"assay.tool.decision","data":{"reason_code":"ALLOW_BY_POLICY","approval_state":"granted","principal":"user:alice"}}
{"type":"assay.process.exec","data":{"hits":1}}
```

That is enough for the shipped rules because:

- `A1-002` passes when **at least one** event contains `reason_code` or
  `approval_state`, typically on `assay.tool.decision`
- `A3-001` passes when **at least one** event contains `principal` or
  `approval_state`, typically on `assay.tool.decision`
- `A5-001` passes when **at least one** event matches `assay.process.exec`

What the pack proves:

- governance rationale fields are present
- authorization context is present
- process-execution evidence is present

What the pack does **not** prove:

- goal hijack detection
- privilege abuse prevention
- mandate-linkage checks
- execution authorization, containment, or sandboxing

For the exact shipped rule wording, see the pack README in
[`packs/open/owasp-agentic-control-evidence-baseline/README.md`](../../packs/open/owasp-agentic-control-evidence-baseline/README.md).

## Step 3: Verify The Release Offline

Assay releases now ship a proof kit that is the canonical consumer verification
path for release provenance.

Unpack the proof kit and verify the downloaded release archives:

```bash
tar -xzf assay-vX.Y.Z-release-proof-kit.tar.gz
cd release-proof-kit
./verify-offline.sh --assets-dir /path/to/release-assets
echo $?
```

Expected behavior:

- success returns exit code `0`
- failure returns nonzero and explains the missing requirement

Representative failure messages:

```text
trusted root not found: /path/to/release-proof-kit/trusted_root.jsonl
bundle not found for asset assay-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz: /path/to/release-proof-kit/bundles/assay-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz.jsonl
asset not found for offline verification: /path/to/release-assets/assay-vX.Y.Z-x86_64-unknown-linux-gnu.tar.gz
```

The offline helper is a thin wrapper around `gh attestation verify`. It does
not invent a second policy and it does not fall back to online verification.
The canonical verification path for this kit is `verify-offline.sh`.

The proof kit inventory is:

- `manifest.json`
- `release-provenance.json`
- `release-provenance.json.sha256`
- `trusted_root.jsonl`
- `bundles/*.jsonl`
- `verify-offline.sh`
- `verify-release-online.sh`
- `README.md`

Release coverage is canonical:

- the release archives covered by the proof kit are the exact top-level
  `.tar.gz` and `.zip` assets selected by
  [`release_archive_inventory.sh`](../../scripts/ci/release_archive_inventory.sh)
- the per-release source of truth for those assets is `manifest.json` `assets[]`

For the full trust boundary and non-goals, see
[Release Proof Kit](../security/RELEASE-PROOF-KIT.md).

## Canonical Summary

| Stage | Input | Command | Practical Result |
| --- | --- | --- | --- |
| Trace normalization | MCP Streamable HTTP transcript | `assay import --format streamable-http session.json --out-trace traces/session.jsonl` | Assay turns transport-specific MCP logs into one canonical trace shape. |
| Evidence interpretation | Evidence bundle | `assay evidence lint --pack owasp-agentic-control-evidence-baseline ...` | Assay applies the shipped control-evidence subset and tells you which fields or event types are missing. |
| Release verification | Release archives + proof kit | `./verify-offline.sh --assets-dir /path/to/release-assets` | A consumer can reproduce the release provenance check offline under the same policy enforced in CI. |

That is the operator story:

1. Assay understands what happened.
2. Assay applies a bounded security interpretation to the evidence.
3. Assay lets consumers verify the shipped release with the same provenance
   policy used in CI.

## Next Steps

- [MCP Quick Start](../mcp/quickstart.md)
- [Release Proof Kit](../security/RELEASE-PROOF-KIT.md)
- [Guides Index](./index.md)
