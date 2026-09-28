# From an installed release to verified denial evidence

Install the selected release using the [installation guide](../getting-started/installation.md),
then run `assay --version`. The Unix channel is `curl -fsSL https://getassay.dev/install.sh | sh`;
select an explicit release by setting `ASSAY_VERSION` on the `sh` side of the pipe.
The installer supplies the CLI only. The companion server and example files below come from
that same version's platform archive. Python 3 runs the local mock; no provider credentials
or source checkout are used. Cosign remains optional for the installer: without it, the
installer checks the archive sidecar and logs that signed-manifest verification was skipped.

## Acquire the companion and example

In a fresh directory, save this standalone Python 3 script as `acquire.py`.
It bounds archive and sidecar downloads before writing beyond their limits and checks the
per-archive sidecar. It does not authenticate provenance. The hosted proof additionally
compares the downloaded archive with release-verified bytes and safely validates/materializes
it with decoded-size and member-count ceilings before running the opening commands below.

<!-- assay-route: download-python -->
```python
import pathlib
import urllib.request


class DownloadRejected(ValueError):
    pass


def download(url: str, destination: pathlib.Path, *, max_bytes: int) -> None:
    if max_bytes <= 0:
        raise ValueError("download ceiling must be positive")
    if destination.exists():
        raise DownloadRejected("download destination already exists")
    scratch = destination.with_name(f".{destination.name}.downloading")
    if scratch.exists():
        raise DownloadRejected("download scratch destination already exists")

    request = urllib.request.Request(
        url,
        headers={"Accept": "application/octet-stream", "User-Agent": "assay-release-verifier"},
    )
    try:
        # The driver validates the exact GitHub release URL before this call.
        with urllib.request.urlopen(request, timeout=60) as response:  # noqa: S310
            declared = response.headers.get("Content-Length")
            if declared is not None and (not declared.isdigit() or int(declared) > max_bytes):
                raise DownloadRejected("download content length exceeds ceiling")
            total = 0
            with scratch.open("xb") as output:
                while chunk := response.read(min(65536, max_bytes - total + 1)):
                    total += len(chunk)
                    if total > max_bytes:
                        raise DownloadRejected("download stream exceeds ceiling")
                    output.write(chunk)
            if total == 0:
                raise DownloadRejected("download yielded no bytes")
        scratch.replace(destination)
    except BaseException:
        scratch.unlink(missing_ok=True)
        raise


import hashlib
import json
import platform
import subprocess

version = subprocess.check_output(["assay", "version"], text=True).strip()
targets = {
    ("Linux", "x86_64"): "x86_64-unknown-linux-gnu",
    ("Linux", "aarch64"): "aarch64-unknown-linux-gnu",
    ("Linux", "arm64"): "aarch64-unknown-linux-gnu",
    ("Darwin", "arm64"): "aarch64-apple-darwin",
    ("Darwin", "x86_64"): "x86_64-apple-darwin",
    ("Windows", "AMD64"): "x86_64-pc-windows-msvc",
}
target = targets[(platform.system(), platform.machine())]
extension = ".zip" if platform.system() == "Windows" else ".tar.gz"
asset = "assay-v" + version + "-" + target + extension
url = "https://github.com/Rul1an/assay/releases/download/v" + version + "/" + asset
download(url, pathlib.Path(asset), max_bytes=134217728)
download(url + ".sha256", pathlib.Path(asset + ".sha256"), max_bytes=16384)
with open(asset, "rb") as stream:
    observed = hashlib.sha256()
    for chunk in iter(lambda: stream.read(65536), b""):
        observed.update(chunk)
    digest = observed.hexdigest()
if pathlib.Path(asset + ".sha256").read_text().strip() != digest + "  " + asset:
    raise ValueError("Archive sidecar mismatch")
pathlib.Path("acquisition.json").write_text(json.dumps({"archive": asset}))
```

On Linux and macOS, use Bash:

<!-- assay-route: acquire-unix -->
```bash
set -euo pipefail
python3 acquire.py
```

Then open the checked archive:

<!-- assay-route: open-unix -->
```bash
ASSET="$(python3 -c 'import json; print(json.load(open("acquisition.json"))["archive"])')"
tar -xzf "$ASSET"
RELEASE_DIR="$PWD/${ASSET%.tar.gz}"
SERVER="$RELEASE_DIR/assay-mcp-server"
EXAMPLE="$RELEASE_DIR/packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate"
"$SERVER" --version
```

On Windows x86_64, use PowerShell. This remains the archive channel, not a Windows
`curl | sh` claim:

<!-- assay-route: acquire-windows -->
```powershell
$ErrorActionPreference = 'Stop'
python acquire.py
if ($LASTEXITCODE -ne 0) { throw 'Bounded archive acquisition failed' }
```

Then open the checked archive:

<!-- assay-route: open-windows -->
```powershell
$Asset = (Get-Content acquisition.json -Raw | ConvertFrom-Json).archive
Expand-Archive $Asset -DestinationPath .
$ReleaseDir = Join-Path $PWD ($Asset -replace '\.zip$', '')
$Server = Join-Path $ReleaseDir 'assay-mcp-server.exe'
$Example = Join-Path $ReleaseDir 'packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate'
& $Server --version
if ($LASTEXITCODE -ne 0) { throw 'Companion version failed' }
```

## Generate real project and proxy records

Stay in the same shell session so `SERVER` and `EXAMPLE` (PowerShell: `$Server` and
`$Example`) remain set. Create and enter a fresh project directory, then run these commands
with the installed CLI. In PowerShell, check `$LASTEXITCODE` after each native command
and stop if it is nonzero.

<!-- assay-route: cli-start -->
```sh
assay init --preset dev --hello-trace
assay doctor --config eval.yaml --format json
assay policy validate --input policy.yaml --format json
assay run --config eval.yaml --trace-file traces/hello.jsonl --format json
```

The following small Python driver works on all five platforms and keeps proxy stdin open
until the denied call is observed. Save it as `deny.py` in that project directory. It uses
the released archive's example files, not files from a source checkout.

<!-- assay-route: deny-python -->
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

On Unix:

<!-- assay-route: deny-unix -->
```sh
python3 deny.py "$SERVER" "$EXAMPLE"
```

In PowerShell:

<!-- assay-route: deny-windows -->
```powershell
python deny.py "$Server" "$Example"
```

The mock is local; a denied request
is not a claim about a real provider action. Keep `decisions.ndjson` and `denied-observations.ndjson`.

## Import, inspect and verify the produced bundle

Run in the directory containing those records:

<!-- assay-route: cli-evidence -->
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

Project the same decisions on Unix:

<!-- assay-route: sarif-unix -->
```sh
"$SERVER" enforcement-sarif --input decisions.ndjson --output enforcement.sarif
```

In PowerShell:

<!-- assay-route: sarif-windows -->
```powershell
& $Server enforcement-sarif --input decisions.ndjson --output enforcement.sarif
```

Require exit 0 and SARIF version 2.1.0. This output describes the recorded
policy decision, not a whole-action verdict.
