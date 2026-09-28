# From an installed release to verified denial evidence

Install the selected release using the [installation guide](../getting-started/installation.md),
then run `assay --version`. The Unix channel is `curl -fsSL https://getassay.dev/install.sh | sh`;
select an explicit release by setting `ASSAY_VERSION` on the `sh` side of the pipe.
The installer supplies the CLI only. The companion server and example files below come from
that same version's platform archive. Python 3 runs the local mock; no provider credentials
or source checkout are used. Cosign remains optional for the installer: without it, the
installer checks the archive sidecar and logs that signed-manifest verification was skipped.

## Acquire the companion and example

On Linux and macOS, in a fresh directory, use Bash:

```bash
set -euo pipefail
VERSION="$(assay version)"
case "$(uname -s)/$(uname -m)" in
  Linux/x86_64) TARGET=x86_64-unknown-linux-gnu ;;
  Linux/aarch64|Linux/arm64) TARGET=aarch64-unknown-linux-gnu ;;
  Darwin/arm64) TARGET=aarch64-apple-darwin ;;
  Darwin/x86_64) TARGET=x86_64-apple-darwin ;;
  *) echo 'Unsupported platform' >&2; exit 1 ;;
esac
ASSET="assay-v${VERSION}-${TARGET}.tar.gz"
URL="https://github.com/Rul1an/assay/releases/download/v${VERSION}/${ASSET}"
curl -fSL "$URL" -o "$ASSET"
curl -fSL "$URL.sha256" -o "$ASSET.sha256"
if command -v sha256sum >/dev/null; then
  sha256sum -c "$ASSET.sha256"
else
  shasum -a 256 -c "$ASSET.sha256"
fi
tar -xzf "$ASSET"
RELEASE_DIR="$PWD/assay-v${VERSION}-${TARGET}"
SERVER="$RELEASE_DIR/assay-mcp-server"
EXAMPLE="$RELEASE_DIR/packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate"
"$SERVER" --version
```

This companion-download recipe checks the per-archive sidecar. It does not claim signature
or authenticated provenance verification. The hosted proof additionally checks the release
API digest, signed manifest and commit-bound archive attestation.

On Windows x86_64, use PowerShell in a fresh directory. This is the archive installation
channel, not a Windows `curl | sh` claim:

```powershell
$ErrorActionPreference = 'Stop'
$Version = (assay version).Trim()
if ($LASTEXITCODE -ne 0) { throw 'Could not read installed CLI version' }
$Target = 'x86_64-pc-windows-msvc'
$Asset = "assay-v$Version-$Target.zip"
$Url = "https://github.com/Rul1an/assay/releases/download/v$Version/$Asset"
Invoke-WebRequest $Url -OutFile $Asset
Invoke-WebRequest "$Url.sha256" -OutFile "$Asset.sha256"
$Expected = (Get-Content "$Asset.sha256" -Raw).Trim()
$Actual = (Get-FileHash $Asset -Algorithm SHA256).Hash.ToLowerInvariant()
if ($Expected -cne "$Actual  $Asset") { throw 'Archive sidecar mismatch' }
Expand-Archive $Asset -DestinationPath .
$ReleaseDir = Join-Path $PWD "assay-v$Version-$Target"
$Server = Join-Path $ReleaseDir 'assay-mcp-server.exe'
$Example = Join-Path $ReleaseDir 'packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate'
& $Server --version
if ($LASTEXITCODE -ne 0) { throw 'Companion version failed' }
```

## Generate real project and proxy records

In a fresh project directory run these commands with the installed CLI. In PowerShell,
check `$LASTEXITCODE` after each native command and stop if it is nonzero.

```sh
assay init --preset dev --hello-trace
assay doctor --config eval.yaml --format json
assay policy validate --input policy.yaml --format json
assay run --config eval.yaml --trace-file traces/hello.jsonl --format json
```

The following small Python driver works on all five platforms and keeps proxy stdin open
until the denied call is observed. Save it as `deny.py` in that project directory. It uses
the released archive's example files, not files from a source checkout.

```python
import json
import pathlib
import queue
import subprocess
import sys
import threading

server, example = sys.argv[1:]
example = pathlib.Path(example).resolve()
argv = [server, "proxy-enforce", "--upstream-command", sys.executable,
        "--upstream-arg", "-u", "--upstream-arg", str(example / "mock_github_mcp.py"),
        "--enforce-policy", str(example / "policies/no-allowance.yaml"),
        "--declared-mcp-manifest", str(example / "baseline-approved.json"),
        "--enforcement-decision-out", "decisions.ndjson",
        "--denied-call-observation-out", "denied-observations.ndjson"]
messages = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2024-11-05", "capabilities": {},
        "clientInfo": {"name": "installed-release-example", "version": "1"}}},
    {"jsonrpc": "2.0", "id": 9, "method": "tools/call", "params": {
        "name": "github.add_deploy_key", "arguments": {"owner": "acme", "repo": "prod-app"}}},
]
with open("proxy.stderr", "w") as errors:
    child = subprocess.Popen(argv, stdin=subprocess.PIPE, stdout=subprocess.PIPE,
                             stderr=errors, text=True)
    replies = queue.Queue()
    def read_replies():
        for line in child.stdout:
            replies.put(line)
        replies.put(None)
    threading.Thread(target=read_replies, daemon=True).start()
    try:
        for message in messages:
            child.stdin.write(json.dumps(message) + "\n")
        child.stdin.flush()
        while True:
            line = replies.get(timeout=30)
            if line is None:
                raise RuntimeError("proxy ended without the denied response")
            response = json.loads(line)
            if response.get("id") == 9:
                assert response["error"]["code"] == -31999
                assert response["error"]["data"]["reason"] == "no_declared_allowance"
                break
        child.stdin.close()
        assert child.wait(timeout=10) == 0
    finally:
        if child.poll() is None:
            child.kill()
        child.wait()
```

Run `python3 deny.py "$SERVER" "$EXAMPLE"` on Unix, or
`python deny.py "$Server" "$Example"` in PowerShell. The mock is local; a denied request
is not a claim about a real provider action. Keep `decisions.ndjson` and `denied-observations.ndjson`.

## Import, inspect and verify the produced bundle

Run in the directory containing those records:

```sh
assay evidence import privileged-mcp-action --decisions decisions.ndjson --denied-observations denied-observations.ndjson --bundle-out action.bundle.tar.gz
assay evidence show --format json -- action.bundle.tar.gz
assay evidence verify-privileged-mcp-action action.bundle.tar.gz --profile-version v1 --format json
```

The successful verification returns exit 0, `bundle_integrity: pass` and `verdict: valid`.
It requires no network access; the hosted journey additionally verifies under its platform's
network-denial constructor and compares the result byte-for-byte with the connected result.

Profile selection is distinct from report schema: the report still uses
`assay.privileged_mcp_action.verify.report.v0`. Proxy denial observations in this release use
`assay.denied_call_observation.v1`, so this bundle needs explicit profile v1. Omitting
`--profile-version v1` selects the compatibility v0 profile and returns exit 2 with
`E_EVIDENCE_PROFILE_INVALID` on this same denial bundle. That refusal is expected, not an
integrity failure. Decisions-only allow bundles can verify under either profile; they do
not establish caller-visible denial, upstream delivery or an external side effect.

Project the same decisions using `"$SERVER" enforcement-sarif --input decisions.ndjson --output enforcement.sarif`
on Unix or `& $Server enforcement-sarif --input decisions.ndjson --output enforcement.sarif`
in PowerShell. Require exit 0 and SARIF version 2.1.0. This output describes the recorded
policy decision, not a whole-action verdict.
