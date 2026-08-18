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
  echo "usage: published-release-golden-path.sh --release-tag vX.Y.Z --harness-sha <40-hex> --workflow-run-id <id> --workflow-run-attempt <n> --run-root <abs-path>" >&2
  exit 2
}

release_tag=""
harness_sha=""
run_root=""
workflow_run_id=""
workflow_run_attempt=""
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
    --workflow-run-id)
      [[ "$#" -ge 2 ]] || usage
      workflow_run_id="$2"
      shift 2
      ;;
    --workflow-run-attempt)
      [[ "$#" -ge 2 ]] || usage
      workflow_run_attempt="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release tag must be an exact stable vX.Y.Z tag"
[[ "$harness_sha" =~ ^[0-9a-f]{40}$ ]] || fail "harness SHA must be exactly 40 lowercase hex characters"
[[ "$workflow_run_id" =~ ^[A-Za-z0-9_.-]+$ ]] || fail "workflow run id has an unsafe shape"
[[ "$workflow_run_attempt" =~ ^[0-9]+$ ]] || fail "workflow run attempt must be numeric"
[[ "$run_root" = /* ]] || fail "run root must be absolute"
[[ ! -e "$run_root" ]] || fail "run root already exists; refusing to reuse prior evidence: $run_root"
[[ -f "$HARNESS_MANIFEST" ]] || fail "harness manifest is missing"

for required in "$GH_BIN" "$JQ_BIN" "$PYTHON_BIN" sha256sum; do
  command -v "$required" >/dev/null 2>&1 || fail "missing required command: $required"
done

install_root="$run_root/install"
harness_root="$run_root/harness"
session_root="$run_root/session"
results="$run_root/results"
downloads="$results/release-assets"
mkdir -p "$downloads" "$install_root/bin" "$harness_root" "$session_root" "$results/attestation-raw"

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
    executable = item.get("executable", False)
    if not isinstance(executable, bool):
        raise SystemExit(f"invalid executable flag: {relative}")
    if executable:
        destination.chmod(0o755)
    report.append({"path": str(relative), "sha256": digest, "executable": executable})
report_path.write_text(
    json.dumps({"schema": manifest["schema"], "files": report}, indent=2, sort_keys=True) + "\n",
    encoding="utf-8",
)
PY

version="${release_tag#v}"
release_api="$results/release-api.json"
"$GH_BIN" api "repos/${REPO}/releases/tags/${release_tag}" >"$release_api"
"$JQ_BIN" -e '.draft == false and .prerelease == false' "$release_api" >/dev/null \
  || fail "release tag is still draft or prerelease"
cli_asset="assay-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"
mcp_asset="assay-mcp-server-${release_tag}-x86_64-unknown-linux-gnu.tar.gz"

tag_ref="$results/tag-ref.json"
"$GH_BIN" api "repos/${REPO}/git/ref/tags/${release_tag}" >"$tag_ref"
source_type="$($JQ_BIN -er '.object.type' "$tag_ref")"
source_digest="$($JQ_BIN -er '.object.sha' "$tag_ref")"
for depth in 1 2 3 4; do
  [[ "$source_type" != "tag" ]] || {
    tag_object="$results/tag-object-${depth}.json"
    "$GH_BIN" api "repos/${REPO}/git/tags/${source_digest}" >"$tag_object"
    source_type="$($JQ_BIN -er '.object.type' "$tag_object")"
    source_digest="$($JQ_BIN -er '.object.sha' "$tag_object")"
  }
done
[[ "$source_type" == "commit" && "$source_digest" =~ ^[0-9a-f]{40}$ ]] \
  || fail "release tag does not resolve to a commit within four tag objects"

download_release_asset() {
  local asset_name="$1" max_bytes="$2"
  local count api_size api_digest asset_url actual_size actual_digest
  count="$($JQ_BIN -er --arg name "$asset_name" '[.assets[] | select(.name == $name)] | length' "$release_api")"
  [[ "$count" -eq 1 ]] || fail "release must contain exactly one asset named $asset_name"
  api_size="$($JQ_BIN -er --arg name "$asset_name" '.assets[] | select(.name == $name) | .size' "$release_api")"
  api_digest="$($JQ_BIN -er --arg name "$asset_name" '.assets[] | select(.name == $name) | .digest' "$release_api")"
  asset_url="$($JQ_BIN -er --arg name "$asset_name" '.assets[] | select(.name == $name) | .browser_download_url' "$release_api")"
  [[ "$api_size" =~ ^[0-9]+$ && "$api_size" -gt 0 && "$api_size" -le "$max_bytes" ]] \
    || fail "release asset exceeds compressed-size ceiling: $asset_name"
  [[ "$api_digest" =~ ^sha256:[0-9a-f]{64}$ ]] || fail "release API omitted asset sha256: $asset_name"
  [[ "$asset_url" == "https://github.com/${REPO}/releases/download/${release_tag}/${asset_name}" ]] \
    || fail "release API returned an unexpected asset URL: $asset_name"
  PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
    'import pathlib,sys; from bounded_download import download; download(sys.argv[1], pathlib.Path(sys.argv[2]), max_bytes=int(sys.argv[3]))' \
    "$asset_url" "$downloads/$asset_name" "$max_bytes"
  actual_size="$(wc -c <"$downloads/$asset_name" | tr -d ' ')"
  [[ "$actual_size" -eq "$api_size" && "$actual_size" -le "$max_bytes" ]] \
    || fail "downloaded asset size differs or exceeds its ceiling: $asset_name"
  actual_digest="sha256:$(sha256sum "$downloads/$asset_name" | cut -d' ' -f1)"
  [[ "$actual_digest" == "$api_digest" ]] || fail "downloaded asset digest differs: $asset_name"
  record_command "download-release-asset" 0 bounded_download "$asset_url" "$downloads/$asset_name" "$max_bytes"
}

download_release_asset "$cli_asset" 67108864
download_release_asset "$mcp_asset" 33554432

# Execute reviewed harness code, not a script carried inside a mutable release asset.
signer_workflow="$REPO/.github/workflows/release.yml"
if ! GH_BIN="$GH_BIN" JQ_BIN="$JQ_BIN" \
  ASSETS_DIR="$downloads" \
  OUT_SUMMARY="$results/attestation-summary.json" \
  OUT_RAW_DIR="$results/attestation-raw" \
  REPO="$REPO" \
  SIGNER_WORKFLOW="$signer_workflow" \
  SOURCE_REF="" \
  SOURCE_DIGEST="$source_digest" \
  bash "$harness_root/scripts/ci/release_attestation_enforce.sh" \
  >"$results/attestation-verify.log" 2>&1; then
  cat "$results/attestation-verify.log" >&2
  fail "reviewed release attestation verifier rejected the published assets"
fi
record_command "verify-release-attestations" 0 "$harness_root/scripts/ci/release_attestation_enforce.sh"

cli_extract="$run_root/cli-extract"
mcp_extract="$run_root/mcp-extract"
safe_extract() {
  PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
    'import pathlib,sys; from safe_extract_release_archive import extract_archive; extract_archive(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), max_decoded_bytes=int(sys.argv[3]))' \
    "$1" "$2" "$3"
}
safe_extract "$downloads/$cli_asset" "$cli_extract" 134217728
safe_extract "$downloads/$mcp_asset" "$mcp_extract" 67108864
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
[[ "$(tr -d '\r\n' <"$results/mcp-version.txt")" == "assay-mcp-server $version" ]] \
  || fail "assay-mcp-server version differs from pinned release"

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
proxy_argv=(
  assay-mcp-server proxy-enforce
  --upstream-command "$PYTHON_BIN" --upstream-arg -u --upstream-arg "$fixture_dir/mock_github_mcp.py"
  --enforce-policy "$fixture_dir/policies/no-allowance.yaml"
  --declared-mcp-manifest "$fixture_dir/baseline-approved.json"
  --enforcement-decision-out "$decisions"
  --denied-call-observation-out "$observations"
)
printf '%s\n%s\n' "$init_request" "$call_request" \
  | "${proxy_argv[@]}" \
      >"$results/proxy.jsonl" 2>"$results/proxy.stderr" || proxy_status=$?
record_command "proxy-enforce" "$proxy_status" "${proxy_argv[@]}"
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
    --bundle-out "$bundle" --run-id "published-release-${workflow_run_id}-${workflow_run_attempt}" \
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
"$PYTHON_BIN" - "$release_tag" "$source_digest" "$results/attestation-summary.json" \
  "$harness_sha" "$workflow_run_id" "$workflow_run_attempt" "$driver_digest" \
  "$harness_manifest_digest" "$results/harness-files.json" "$commands_file" "$results/run-pin.json" <<'PY'
import json, pathlib, sys
(release_tag, source_digest, attestation_summary_path, harness_sha, workflow_run_id,
 workflow_run_attempt, driver_digest, harness_manifest_digest, harness_files_path,
 commands_path, output_path) = sys.argv[1:]
attestations = json.loads(pathlib.Path(attestation_summary_path).read_text(encoding="utf-8"))
harness = json.loads(pathlib.Path(harness_files_path).read_text(encoding="utf-8"))
commands = [json.loads(line) for line in pathlib.Path(commands_path).read_text(encoding="utf-8").splitlines() if line]
document = {
    "schema": "assay.published_release_golden_path.run_pin.v1",
    "release": {
        "tag": release_tag,
        "source_ref": f"refs/tags/{release_tag}",
        "source_digest": source_digest,
        "assets": [{"name": row["name"], "sha256": row["sha256"]} for row in attestations["assets"]],
    },
    "harness": {
        "head_sha": harness_sha,
        "workflow_run_id": workflow_run_id,
        "workflow_run_attempt": int(workflow_run_attempt),
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
    "release-api.json", "tag-ref.json", "attestation-summary.json",
]
for name in required:
    path = root / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"required retained artifact is missing or empty: {name}")
for pattern, expected in (("release-assets/*.tar.gz", 2), ("attestation-raw/*.json", 2)):
    matches = list(root.glob(pattern))
    if len(matches) != expected or any(path.stat().st_size == 0 for path in matches):
        raise SystemExit(f"retained trust inputs for {pattern} are incomplete")
files = []
for path in sorted(root.rglob("*")):
    if path.is_file() and path != output:
        files.append({
            "path": path.relative_to(root).as_posix(),
            "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
            "size": path.stat().st_size,
        })
document = {"schema": "assay.published_release_golden_path.artifacts.v1", "files": files}
output.write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "PASS: published release ${release_tag} completed the install-to-verified-evidence journey"
