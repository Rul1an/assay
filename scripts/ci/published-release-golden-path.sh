#!/usr/bin/env bash
# jq programs intentionally use single quotes so shell variables are not expanded.
# shellcheck disable=SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPO="${GITHUB_REPOSITORY:-Rul1an/assay}"
GH_BIN="${GH_BIN:-gh}"
JQ_BIN="${JQ_BIN:-jq}"
PYTHON_BIN="${PYTHON_BIN:-python3}"
HARNESS_MANIFEST="${ROOT}/scripts/ci/fixtures/published-release-golden-path/v1/harness-manifest.json"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

usage() {
  echo "usage: published-release-golden-path.sh --release-tag vX.Y.Z --harness-sha <40-hex> --run-root <abs-path>" >&2
  exit 2
}

release_tag=""
harness_sha=""
run_root=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --release-tag)
      [[ "$#" -ge 2 ]] || usage
      release_tag="$2"
      shift 2
      ;;
    --harness-sha)
      [[ "$#" -ge 2 ]] || usage
      harness_sha="$2"
      shift 2
      ;;
    --run-root)
      [[ "$#" -ge 2 ]] || usage
      run_root="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release tag must be an exact stable vX.Y.Z tag"
[[ "$harness_sha" =~ ^[0-9a-f]{40}$ ]] || fail "harness SHA must be exactly 40 lowercase hex characters"
[[ "$run_root" = /* ]] || fail "run root must be absolute"
[[ ! -e "$run_root" ]] || fail "run root already exists; refusing to reuse prior evidence: $run_root"
[[ -f "$HARNESS_MANIFEST" ]] || fail "harness manifest is missing"

for required in "$GH_BIN" "$JQ_BIN" "$PYTHON_BIN" sha256sum tar; do
  command -v "$required" >/dev/null 2>&1 || fail "missing required command: $required"
done

downloads="$run_root/downloads"
proof_root="$run_root/proof"
install_root="$run_root/install"
harness_root="$run_root/harness"
session_root="$run_root/session"
results="$run_root/results"
mkdir -p "$downloads" "$proof_root" "$install_root/bin" "$harness_root" "$session_root" "$results"

commands_file="$results/commands.ndjson"
: >"$commands_file"

record_command() {
  local name="$1" status="$2"
  shift 2
  "$PYTHON_BIN" - "$commands_file" "$name" "$status" "$@" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
record = {"name": sys.argv[2], "exit_code": int(sys.argv[3]), "argv": sys.argv[4:]}
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

run_capture() {
  local name="$1" expected="$2" stdout_path="$3" stderr_path="$4"
  shift 4
  local status=0
  "$@" >"$stdout_path" 2>"$stderr_path" || status=$?
  record_command "$name" "$status" "$@"
  [[ "$status" -eq "$expected" ]] || {
    cat "$stderr_path" >&2
    fail "$name exited $status, expected $expected"
  }
}

# Stage only manifest-listed harness bytes, verify each digest first, and retain what was used.
"$PYTHON_BIN" - "$HARNESS_MANIFEST" "$ROOT" "$harness_root" "$results/harness-files.json" <<'PY'
import hashlib, json, pathlib, shutil, sys

manifest_path, root_path, output_path, report_path = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
if manifest.get("schema") != "assay.published_release_golden_path.harness.v1":
    raise SystemExit("unexpected harness manifest schema")
files = manifest.get("files")
if not isinstance(files, list) or not files:
    raise SystemExit("harness manifest contains no files")
report = []
for item in files:
    relative = pathlib.PurePosixPath(item["path"])
    if relative.is_absolute() or ".." in relative.parts or "." in relative.parts:
        raise SystemExit(f"unsafe harness path: {relative}")
    source = root_path.joinpath(*relative.parts)
    data = source.read_bytes()
    digest = hashlib.sha256(data).hexdigest()
    if digest != item.get("sha256"):
        raise SystemExit(f"harness digest mismatch: {relative}")
    destination = output_path.joinpath(*relative.parts)
    destination.parent.mkdir(parents=True, exist_ok=True)
    destination.write_bytes(data)
    report.append({"path": str(relative), "sha256": digest})
report_path.write_text(
    json.dumps({"schema": manifest["schema"], "files": report}, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

version="${release_tag#v}"
proof_asset="assay-${release_tag}-release-proof-kit.tar.gz"
release_api="$results/release-api.json"
"$GH_BIN" api "repos/${REPO}/releases/tags/${release_tag}" >"$release_api"
"$GH_BIN" release download "$release_tag" --repo "$REPO" --dir "$downloads" --pattern "$proof_asset" --clobber

proof_digest="$($JQ_BIN -er --arg name "$proof_asset" '.assets[] | select(.name == $name) | .digest' "$release_api")"
[[ "$proof_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || fail "release API omitted the proof-kit sha256 digest"
actual_proof_digest="sha256:$(sha256sum "$downloads/$proof_asset" | cut -d' ' -f1)"
[[ "$actual_proof_digest" == "$proof_digest" ]] || fail "proof-kit digest differs from the release API"

tar -xzf "$downloads/$proof_asset" -C "$proof_root"
kit_root="$proof_root/release-proof-kit"
kit_manifest="$kit_root/manifest.json"
[[ -x "$kit_root/verify-offline.sh" ]] || fail "release proof kit has no executable offline verifier"
[[ "$($JQ_BIN -er '.tag' "$kit_manifest")" == "$release_tag" ]] || fail "proof-kit tag differs from requested release"
[[ "$($JQ_BIN -er '.repo' "$kit_manifest")" == "$REPO" ]] || fail "proof-kit repository differs from requested repository"
source_digest="$($JQ_BIN -er '.source_digest' "$kit_manifest")"
[[ "$source_digest" =~ ^[0-9a-f]{40}$ ]] || fail "proof-kit source digest is not a commit SHA"

while IFS= read -r asset_name; do
  "$GH_BIN" release download "$release_tag" --repo "$REPO" --dir "$downloads" --pattern "$asset_name" --clobber
  expected_api_digest="$($JQ_BIN -er --arg name "$asset_name" '.assets[] | select(.name == $name) | .digest' "$release_api")"
  expected_kit_digest="sha256:$($JQ_BIN -er --arg name "$asset_name" '.assets[] | select(.name == $name) | .sha256' "$kit_manifest")"
  actual_digest="sha256:$(sha256sum "$downloads/$asset_name" | cut -d' ' -f1)"
  [[ "$expected_api_digest" == "$expected_kit_digest" ]] || fail "release API and proof kit disagree for $asset_name"
  [[ "$actual_digest" == "$expected_kit_digest" ]] || fail "downloaded asset digest differs for $asset_name"
done < <("$JQ_BIN" -er '.assets[].name' "$kit_manifest")

# This is the release's canonical offline attestation policy. It must complete before installation.
"$kit_root/verify-offline.sh" --assets-dir "$downloads" >"$results/attestation-verify.log" 2>&1
record_command "verify-release-attestations" 0 "$kit_root/verify-offline.sh" --assets-dir "$downloads"

cli_asset="assay-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"
mcp_asset="assay-mcp-server-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"
for asset in "$cli_asset" "$mcp_asset"; do
  "$JQ_BIN" -e --arg name "$asset" '.assets[] | select(.name == $name)' "$kit_manifest" >/dev/null \
    || fail "proof kit does not attest required Linux asset: $asset"
done

cli_extract="$run_root/cli-extract"
mcp_extract="$run_root/mcp-extract"
mkdir -p "$cli_extract" "$mcp_extract"
tar -xzf "$downloads/$cli_asset" -C "$cli_extract"
tar -xzf "$downloads/$mcp_asset" -C "$mcp_extract"
mapfile -t cli_candidates < <(find "$cli_extract" -type f -name assay -perm -u+x)
mapfile -t mcp_candidates < <(find "$mcp_extract" -type f -name assay-mcp-server -perm -u+x)
[[ "${#cli_candidates[@]}" -eq 1 ]] || fail "CLI archive must contain exactly one executable assay binary"
[[ "${#mcp_candidates[@]}" -eq 1 ]] || fail "MCP archive must contain exactly one executable assay-mcp-server binary"
cp "${cli_candidates[0]}" "$install_root/bin/assay"
cp "${mcp_candidates[0]}" "$install_root/bin/assay-mcp-server"
chmod 0755 "$install_root/bin/assay" "$install_root/bin/assay-mcp-server"

export HOME="$run_root/home"
mkdir -p "$HOME"
export PATH="$install_root/bin:/usr/bin:/bin"
[[ "$(command -v assay)" == "$install_root/bin/assay" ]] || fail "assay did not resolve from the disposable install prefix"
[[ "$(command -v assay-mcp-server)" == "$install_root/bin/assay-mcp-server" ]] || fail "assay-mcp-server did not resolve from the disposable install prefix"

run_capture "assay-version" 0 "$results/assay-version.txt" "$results/assay-version.stderr" assay version
[[ "$(tr -d '\r\n' <"$results/assay-version.txt")" == "$version" ]] || fail "assay version differs from pinned release"
run_capture "mcp-version" 0 "$results/mcp-version.txt" "$results/mcp-version.stderr" assay-mcp-server --version
grep -Eq "(^|[[:space:]])${version}$" "$results/mcp-version.txt" || fail "assay-mcp-server version differs from pinned release"

pushd "$session_root" >/dev/null
run_capture "init" 0 "$results/init.json" "$results/init.stderr" assay init --preset dev --hello-trace --format json
"$JQ_BIN" -e '.schema == "assay.init_report.v0"' "$results/init.json" >/dev/null || fail "init output identity drifted"
run_capture "policy-validate" 0 "$results/policy-validate.json" "$results/policy-validate.stderr" \
  assay policy validate --input policy.yaml --format json
"$JQ_BIN" -e '.schema == "assay.run_summary.v1"' "$results/policy-validate.json" >/dev/null || fail "policy validation output identity drifted"
run_capture "evaluate" 0 "$results/evaluate.json" "$results/evaluate.stderr" \
  assay run --config eval.yaml --trace-file traces/hello.jsonl --format json
"$JQ_BIN" -e '.schema == "assay.run_report.v1"' "$results/evaluate.json" >/dev/null || fail "evaluation output identity drifted"
popd >/dev/null

fixture_dir="$harness_root/examples/privileged-action-gate"
decisions="$results/decisions.ndjson"
observations="$results/denied-observations.ndjson"
init_request='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"published-release-gate","version":"1"}}}'
call_request='{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}'
proxy_status=0
printf '%s\n%s\n' "$init_request" "$call_request" \
  | assay-mcp-server proxy-enforce \
      --upstream-command "$PYTHON_BIN" --upstream-arg -u --upstream-arg "$fixture_dir/mock_github_mcp.py" \
      --enforce-policy "$fixture_dir/policies/no-allowance.yaml" \
      --declared-mcp-manifest "$fixture_dir/baseline-approved.json" \
      --enforcement-decision-out "$decisions" \
      --denied-call-observation-out "$observations" \
      >"$results/proxy.jsonl" 2>"$results/proxy.stderr" || proxy_status=$?
record_command "proxy-enforce" "$proxy_status" assay-mcp-server proxy-enforce --upstream-command python3
[[ "$proxy_status" -eq 0 ]] || fail "proxy-enforce exited $proxy_status, expected 0 for a policy denial"
[[ -s "$decisions" ]] || fail "proxy-enforce produced no enforcement decision"
[[ -s "$observations" ]] || fail "proxy-enforce produced no denied-call observation"
"$JQ_BIN" -e 'select(.schema == "assay.enforcement_decision.v0" and .decision == "deny" and .reason == "no_declared_allowance")' "$decisions" >/dev/null \
  || fail "enforcement decision identity or semantics drifted"

bundle="$results/produced.bundle.tar.gz"
[[ ! -e "$bundle" ]] || fail "bundle existed before production"
run_capture "produce-bundle" 0 "$results/produce.stdout" "$results/produce.stderr" \
  assay evidence import privileged-mcp-action \
    --decisions "$decisions" --denied-observations "$observations" \
    --bundle-out "$bundle" --run-id published-release-golden-path \
    --import-time 2026-01-01T00:00:00Z
[[ -s "$bundle" ]] || fail "bundle production yielded no bytes"

run_capture "inspect-produced-bundle" 0 "$results/inspect.json" "$results/inspect.stderr" \
  assay evidence show --format json -- "$bundle"
"$JQ_BIN" -e '.verify_mode == "enabled" and (.manifest | type == "object") and (.events | length > 0)' "$results/inspect.json" >/dev/null \
  || fail "inspection did not verify the produced bundle"
run_capture "verify-produced-bundle" 0 "$results/verify.json" "$results/verify.stderr" \
  assay evidence verify-privileged-mcp-action "$bundle" --format json
"$JQ_BIN" -e '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "pass" and .verdict == "valid"' "$results/verify.json" >/dev/null \
  || fail "profile verification did not validate the produced bundle"

run_capture "export-sarif" 0 "$results/sarif.stdout" "$results/sarif.stderr" \
  assay-mcp-server enforcement-sarif --input "$decisions" --output "$results/enforcement.sarif"
"$JQ_BIN" -e '.version == "2.1.0" and (.runs[0].results | length == 1)' "$results/enforcement.sarif" >/dev/null \
  || fail "SARIF export identity or deny count drifted"

tampered="$results/tampered.bundle.tar.gz"
"$PYTHON_BIN" - "$bundle" "$tampered" <<'PY'
import io, tarfile, sys
source, destination = sys.argv[1:]
with tarfile.open(source, "r:gz") as archive:
    members = archive.getmembers()
    payloads = {}
    for member in members:
        if member.isfile():
            handle = archive.extractfile(member)
            payloads[member.name] = handle.read() if handle else b""
event_name = next((name for name in payloads if name.endswith("events.ndjson")), None)
if event_name is None:
    raise SystemExit("produced bundle has no events.ndjson")
payloads[event_name] += b"\n"
with tarfile.open(destination, "w:gz") as archive:
    for member in members:
        if member.isfile():
            data = payloads[member.name]
            member.size = len(data)
            archive.addfile(member, io.BytesIO(data))
        else:
            archive.addfile(member)
PY
run_capture "verify-tampered-bundle" 2 "$results/tamper-verify.json" "$results/tamper-verify.stderr" \
  assay evidence verify-privileged-mcp-action "$tampered" --format json
"$JQ_BIN" -e '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "fail" and .reason_code == "E_EVIDENCE_INTEGRITY"' "$results/tamper-verify.json" >/dev/null \
  || fail "tampered produced bundle did not fail with E_EVIDENCE_INTEGRITY"

driver_digest="$(sha256sum "$ROOT/scripts/ci/published-release-golden-path.sh" | cut -d' ' -f1)"
harness_manifest_digest="$(sha256sum "$HARNESS_MANIFEST" | cut -d' ' -f1)"
"$PYTHON_BIN" - "$release_tag" "$source_digest" "$kit_manifest" "$harness_sha" "$driver_digest" \
  "$harness_manifest_digest" "$results/harness-files.json" "$commands_file" "$results/run-pin.json" <<'PY'
import json, pathlib, sys
(release_tag, source_digest, kit_manifest_path, harness_sha, driver_digest,
 harness_manifest_digest, harness_files_path, commands_path, output_path) = sys.argv[1:]
kit = json.loads(pathlib.Path(kit_manifest_path).read_text(encoding="utf-8"))
harness = json.loads(pathlib.Path(harness_files_path).read_text(encoding="utf-8"))
commands = [json.loads(line) for line in pathlib.Path(commands_path).read_text(encoding="utf-8").splitlines() if line]
document = {
    "schema": "assay.published_release_golden_path.run_pin.v1",
    "release": {
        "tag": release_tag,
        "source_digest": source_digest,
        "assets": [{"name": row["name"], "sha256": row["sha256"]} for row in kit["assets"]],
    },
    "harness": {
        "head_sha": harness_sha,
        "driver_sha256": driver_digest,
        "manifest_sha256": harness_manifest_digest,
        "files": harness["files"],
    },
    "commands": commands,
    "claim_ceiling": (
        "The attested release binaries completed the bounded Linux x86_64 journey under the "
        "recorded harness head and fixture digests; the harness is not a shipped release asset."
    ),
}
pathlib.Path(output_path).write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

"$PYTHON_BIN" - "$results" "$results/retained-artifacts.json" <<'PY'
import hashlib, json, pathlib, sys
root, output = map(pathlib.Path, sys.argv[1:])
required = [
    "run-pin.json", "commands.ndjson", "produced.bundle.tar.gz", "decisions.ndjson",
    "inspect.json", "verify.json", "tamper-verify.json", "enforcement.sarif",
]
for name in required:
    path = root / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"required retained artifact is missing or empty: {name}")
files = []
for path in sorted(root.iterdir()):
    if path.is_file() and path != output:
        files.append({"path": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest(), "size": path.stat().st_size})
document = {"schema": "assay.published_release_golden_path.artifacts.v1", "files": files}
output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "PASS: published release ${release_tag} completed the install-to-verified-evidence journey"
