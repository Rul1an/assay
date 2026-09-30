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

resolve_required_command() {
  local requested="$1" resolved directory name
  [[ -n "$requested" && "$requested" != -* ]] || fail "required command has an unsafe name"
  resolved="$(type -P "$requested")" || fail "missing required command: $requested"
  [[ -n "$resolved" ]] || fail "missing required command: $requested"
  if [[ "$resolved" != /* ]]; then
    directory="${resolved%/*}"
    name="${resolved##*/}"
    [[ "$directory" != "$resolved" && -d "$directory" ]] \
      || fail "required command did not resolve to a path: $requested"
    directory="$(CDPATH='' cd -P -- "$directory" >/dev/null && pwd -P)" \
      || fail "required command directory is unreadable: $requested"
    resolved="$directory/$name"
  fi
  [[ -x "$resolved" ]] || fail "required command is not executable: $requested"
  printf '%s\n' "$resolved"
}

usage() {
  echo "usage: published-release-golden-path.sh --release-tag vX.Y.Z --harness-sha <40-hex> --workflow-run-id <id> --workflow-run-attempt <n> --run-root <abs-path> [--target <published-triple>]" >&2
  exit 2
}

release_tag=""
harness_sha=""
run_root=""
workflow_run_id=""
workflow_run_attempt=""
target=""
verified_cli_dir=""
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
    --target)
      [[ "$#" -ge 2 ]] || usage
      target="$2"
      shift 2
      ;;
    --verified-cli-dir)
      [[ "$#" -ge 2 ]] || usage
      verified_cli_dir="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ "$release_tag" =~ ^v[0-9]+\.[0-9]+\.[0-9]+$ ]] || fail "release tag must be an exact stable vX.Y.Z tag"
[[ "$harness_sha" =~ ^[0-9a-f]{40}$ ]] || fail "harness SHA must be exactly 40 lowercase hex characters"
[[ "$workflow_run_id" =~ ^[A-Za-z0-9_.-]+$ ]] || fail "workflow run id has an unsafe shape"
[[ "$workflow_run_attempt" =~ ^[0-9]+$ ]] || fail "workflow run attempt must be numeric"
if [[ "$run_root" != /* ]] && command -v cygpath >/dev/null 2>&1; then
  run_root="$(cygpath -u "$run_root")"
fi
if [[ -n "$verified_cli_dir" && "$verified_cli_dir" != /* ]] && command -v cygpath >/dev/null 2>&1; then
  verified_cli_dir="$(cygpath -u "$verified_cli_dir")"
fi
[[ "$run_root" = /* ]] || fail "run root must be absolute"
[[ ! -e "$run_root" ]] || fail "run root already exists; refusing to reuse prior evidence: $run_root"

resolve_linux_target_from_host() {
  case "$(uname -m)" in
    x86_64|amd64) printf '%s\n' "x86_64-unknown-linux-gnu" ;;
    aarch64|arm64) printf '%s\n' "aarch64-unknown-linux-gnu" ;;
    *) fail "unsupported host architecture for published Linux journey: $(uname -m)" ;;
  esac
}

read_proc_translated() {
  "${SYSCTL_BIN:-/usr/sbin/sysctl}" -n sysctl.proc_translated
}

read_hw_optional_arm64() {
  "${SYSCTL_BIN:-/usr/sbin/sysctl}" -n hw.optional.arm64
}

# sysctl's ENOENT text names the queried OID. proc_translated exists only on
# Apple Silicon; a native Intel Mac reports unknown oid 'sysctl.proc_translated'
# instead of 0. A message that names a different OID is not that report.
sysctl_output_is_unknown_oid() {
  local output="$1"
  local oid="$2"
  local needle="unknown oid '${oid}'"
  [[ -n "$oid" && "$output" == *"$needle"* ]]
}

resolve_darwin_target_from_host() {
  local machine arm_status arm_out proc_status proc_out silicon
  machine="$(uname -m)"
  arm_status=0
  arm_out="$(read_hw_optional_arm64 2>&1)" || arm_status=$?
  if [[ "$arm_status" -eq 0 && "$arm_out" == "1" ]]; then
    silicon="apple"
  elif [[ "$machine" == "x86_64" && "$arm_status" -eq 0 && "$arm_out" == "0" ]]; then
    silicon="intel"
  elif [[ "$machine" == "x86_64" && "$arm_status" -ne 0 ]] && sysctl_output_is_unknown_oid "$arm_out" "hw.optional.arm64"; then
    silicon="intel"
  elif [[ "$arm_status" -ne 0 ]]; then
    fail "hw.optional.arm64 is unreadable"
  else
    fail "unexpected Darwin host architecture (uname -m=${machine}, hw.optional.arm64=${arm_out})"
  fi

  proc_status=0
  proc_out="$(read_proc_translated 2>&1)" || proc_status=$?
  if [[ "$silicon" == "intel" && "$proc_status" -ne 0 ]] && sysctl_output_is_unknown_oid "$proc_out" "sysctl.proc_translated"; then
    host_proc_translated=""
  elif [[ "$proc_status" -ne 0 || -z "$proc_out" ]]; then
    fail "sysctl.proc_translated is unreadable"
  else
    host_proc_translated="$proc_out"
  fi
  case "$host_proc_translated" in
    0) ;;
    "")
      [[ "$silicon" == "intel" ]] || fail "sysctl.proc_translated is unreadable"
      ;;
    *) fail "refusing Rosetta-translated process (sysctl.proc_translated=${host_proc_translated})" ;;
  esac

  case "$silicon" in
    apple)
      [[ "$machine" == "arm64" ]] || fail "unexpected Darwin host architecture (uname -m=${machine})"
      host_target="aarch64-apple-darwin"
      ;;
    intel)
      host_target="x86_64-apple-darwin"
      ;;
    *) fail "unexpected Darwin host architecture (uname -m=${machine})" ;;
  esac
}

resolve_host_target() {
  case "$(uname -s)" in
    Linux) host_target="$(resolve_linux_target_from_host)" ;;
    Darwin) resolve_darwin_target_from_host ;;
    MINGW*|MSYS*|CYGWIN*)
      [[ "$(uname -m)" == "x86_64" ]] \
        || fail "unsupported Windows host architecture for published journey: $(uname -m)"
      host_target="x86_64-pc-windows-msvc"
      ;;
    *) fail "unsupported host OS for published journey: $(uname -s)" ;;
  esac
}

# One seam for every published target. Linux keeps the AppArmor sysctl attempt
# and unshare -rn true. Darwin proves sandbox-exec accepts a permissive profile.
# Any other target fails closed.
preflight_offline_constructor() {
  case "$target" in
    x86_64-unknown-linux-gnu|aarch64-unknown-linux-gnu)
      # Ensure unprivileged user namespaces are permitted (e.g. Ubuntu 24.04 AppArmor restriction).
      if ! unshare -rn true >/dev/null 2>&1; then
        if command -v sudo >/dev/null 2>&1; then
          if sudo PATH="/usr/sbin:/sbin:$PATH" sysctl -w kernel.apparmor_restrict_unprivileged_userns=0 >/dev/null 2>&1; then
            :
          fi
        fi
      fi
      if ! unshare_err="$(unshare -rn true 2>&1)"; then
        fail "unshare -rn is not permitted in this environment: ${unshare_err:-unknown error}"
      fi
      ;;
    aarch64-apple-darwin|x86_64-apple-darwin)
      if ! sandbox_err="$(/usr/bin/sandbox-exec -p '(version 1)(allow default)' true 2>&1)"; then
        fail "sandbox-exec permissive profile was refused: ${sandbox_err:-unknown error}"
      fi
      ;;
    x86_64-pc-windows-msvc)
      [[ -f "$harness_root/scripts/ci/published_release_offline_windows.py" ]] \
        || fail "Windows offline constructor is missing"
      ;;
    *)
      fail "no offline constructor for ${target}"
      ;;
  esac
}

sha256_file() {
  "$PYTHON_BIN" -c 'import hashlib, pathlib, sys; print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())' "$1"
}

select_journey_product_archives() {
  case "$2" in
    x86_64-pc-windows-msvc)
      cli_asset="assay-${1}-${2}.zip"
      mcp_asset=""
      ;;
    *)
      cli_asset="assay-${1}-${2}.tar.gz"
      mcp_asset="$cli_asset"
      ;;
  esac
}

record_published_server_install() {
  "$PYTHON_BIN" - "$results/server-install.json" "$cli_asset" "$install_root/bin/assay-mcp-server" <<'PYDATA'
import hashlib, json, pathlib, sys
output, archive, binary = sys.argv[1:]
pathlib.Path(output).write_text(json.dumps({
    "schema": "assay.published_release_server_install.v1",
    "source_kind": "verified-release-archive-member", "archive": archive,
    "binary_sha256": hashlib.sha256(pathlib.Path(binary).read_bytes()).hexdigest(),
}) + "\n")
PYDATA
}

host_proc_translated=""
host_target=""
resolve_host_target
if [[ -z "$target" ]]; then
  target="$host_target"
elif [[ "$target" != "$host_target" ]]; then
  fail "requested target ${target} does not match host architecture (${host_target})"
fi
case "$target" in
  x86_64-unknown-linux-gnu) platform_claim="Linux x86_64" ;;
  aarch64-unknown-linux-gnu) platform_claim="Linux arm64" ;;
  aarch64-apple-darwin) platform_claim="macOS arm64" ;;
  x86_64-apple-darwin) platform_claim="macOS x86_64" ;;
  x86_64-pc-windows-msvc) platform_claim="Windows x86_64" ;;
  *) fail "unsupported published journey target: ${target}" ;;
esac

[[ -f "$HARNESS_MANIFEST" ]] || fail "harness manifest is missing"

GH_BIN="$(resolve_required_command "$GH_BIN")"
JQ_BIN="$(resolve_required_command "$JQ_BIN")"
PYTHON_BIN="$(resolve_required_command "$PYTHON_BIN")"
"$PYTHON_BIN" -c 'import sys; raise SystemExit(0 if sys.version_info.major == 3 else 1)' \
  || fail "required Python interpreter is not Python 3: $PYTHON_BIN"

install_root="$run_root/install"
harness_root="$run_root/harness"
session_root="$run_root/session"
results="$run_root/results"
downloads="$results/release-assets"
mkdir -p "$downloads" "$install_root/bin" "$harness_root" "$session_root" "$results/attestation-raw"
printf '%s' "$target" >"$results/journey-target.txt"
printf '%s' "$platform_claim" >"$results/journey-platform-claim.txt"
select_journey_product_archives "$release_tag" "$target"
printf '%s' "$cli_asset" >"$results/journey-cli-asset.txt"
if [[ -n "$mcp_asset" ]]; then
  printf '%s' "$mcp_asset" >"$results/journey-mcp-asset.txt"
fi
printf '%s\n' "$(uname -s)" >"$results/host-uname-s.txt"
printf '%s\n' "$(uname -m)" >"$results/host-uname-m.txt"
if [[ -n "$host_proc_translated" ]]; then
  printf '%s\n' "$host_proc_translated" >"$results/sysctl-proc-translated.txt"
fi


commands_file="$results/commands.ndjson"
: >"$commands_file"

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

# Source only the manifest-verified copy, not an unrecorded helper.
# shellcheck source=scripts/ci/lib/published-release-capture.sh
source "$harness_root/scripts/ci/lib/published-release-capture.sh"

version="${release_tag#v}"
release_api="$results/release-api.json"
"$GH_BIN" api "repos/${REPO}/releases/tags/${release_tag}" >"$release_api"
"$JQ_BIN" -e '.draft == false and .prerelease == false' "$release_api" >/dev/null \
  || fail "release tag is still draft or prerelease"

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
printf '%s\n' "$source_digest" >"$results/journey-source-sha.txt"

download_release_asset() {
  local asset_name="$1" max_bytes="$2" preexisting="${3:-}"
  if [[ -n "$preexisting" ]]; then
    local identity expected_digest actual_digest_file
    identity="$(find "$verified_cli_dir" -name certificate-identity.txt -type f)"
    [[ -f "$preexisting" && -f "$preexisting.sha256" && -n "$identity" ]] \
      || fail "platform journey requires the checksum-verified CLI archive"
    [[ "$(printf '%s\n' "$identity" | wc -l | tr -d ' ')" -eq 1 ]] \
      || fail "checksum consumer certificate identity is not unique"
    expected_digest="$(tr -d '[:space:]' <"$preexisting.sha256")"
    actual_digest_file="$(sha256_file "$preexisting")"
    [[ "$actual_digest_file" == "$expected_digest" ]] \
      || fail "verified CLI archive sha256 does not match the checksum consumer"
    cp "$identity" "$results/checksum-consumer-identity.txt"
  fi
  printf '%s\n' "$asset_name" >>"$results/journey-downloaded-assets.txt"
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
  if [[ -n "$preexisting" ]]; then
    cp "$preexisting" "$downloads/$asset_name"
  else
    PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
      'import pathlib,sys; from bounded_download import download; download(sys.argv[1], pathlib.Path(sys.argv[2]), max_bytes=int(sys.argv[3]))' \
      "$asset_url" "$downloads/$asset_name" "$max_bytes"
  fi
  actual_size="$(wc -c <"$downloads/$asset_name" | tr -d ' ')"
  [[ "$actual_size" -eq "$api_size" && "$actual_size" -le "$max_bytes" ]] \
    || fail "downloaded asset size differs or exceeds its ceiling: $asset_name"
  actual_digest="sha256:$(sha256_file "$downloads/$asset_name")"
  [[ "$actual_digest" == "$api_digest" ]] || fail "downloaded asset digest differs: $asset_name"
  record_command "download-release-asset" 0 bounded_download "$asset_url" "$downloads/$asset_name" "$max_bytes"
}

[[ -z "$mcp_asset" || "$mcp_asset" == "$cli_asset" ]] \
  || fail "companion source must be the selected CLI archive"

if [[ "$target" == *-apple-darwin || "$target" == x86_64-pc-windows-msvc ]]; then
  [[ -n "$verified_cli_dir" && "$verified_cli_dir" = /* ]] \
    || fail "platform journey requires --verified-cli-dir"
  verified_matches="$(find "$verified_cli_dir" -name "$cli_asset" -type f)"
  [[ -n "$verified_matches" ]] || fail "checksum-verified CLI archive is missing: $cli_asset"
  [[ "$(printf '%s\n' "$verified_matches" | wc -l | tr -d ' ')" -eq 1 ]] \
    || fail "checksum-verified CLI archive is not unique: $cli_asset"
  download_release_asset "$cli_asset" 67108864 "$verified_matches"
else
  [[ -z "$verified_cli_dir" ]] || fail "Linux journey must not receive a pre-verified CLI archive"
  download_release_asset "$cli_asset" 67108864
fi

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

# Release binaries and the mock upstream do not need repository credentials.
unset GH_TOKEN GITHUB_TOKEN PYTHONPATH

cli_extract="$run_root/cli-extract"
safe_extract() {
  PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
    'import pathlib,sys; from safe_extract_release_archive import extract_archive; extract_archive(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), max_decoded_bytes=int(sys.argv[3]))' \
    "$1" "$2" "$3"
}
safe_extract "$downloads/$cli_asset" "$cli_extract" 134217728
mcp_name="assay-mcp-server"
[[ "$target" != x86_64-pc-windows-msvc ]] || mcp_name="assay-mcp-server.exe"
mcp_candidates=()
while IFS= read -r path; do
  mcp_candidates+=("$path")
done < <(find "$cli_extract" -type f -name "$mcp_name")
[[ "${#mcp_candidates[@]}" -eq 1 ]] || fail "CLI archive must contain exactly one companion server"
cp "${mcp_candidates[0]}" "$install_root/bin/$mcp_name"
if [[ "$target" != x86_64-pc-windows-msvc ]]; then
  record_published_server_install
fi

cli_candidates=()
if [[ "$target" == x86_64-pc-windows-msvc ]]; then
  while IFS= read -r path; do
    cli_candidates+=("$path")
  done < <(find "$cli_extract" -type f -name assay.exe)
  [[ "${#cli_candidates[@]}" -eq 1 ]] || fail "Windows CLI archive must contain exactly one assay.exe"
  cp "${cli_candidates[0]}" "$install_root/bin/assay.exe"
  assay_path="$install_root/bin/assay.exe"
  mcp_path="$install_root/bin/assay-mcp-server.exe"
else
  "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_installer.py"
  assay_path="$install_root/bin/assay"
  mcp_path="$install_root/bin/assay-mcp-server"
  chmod 0755 "$assay_path" "$mcp_path"
fi

packaged_fixture="$cli_extract/assay-${release_tag}-${target}/packaging/agent-plugin/skills/assay-golden-path/assets/privileged-action-gate"
[[ -d "$packaged_fixture" ]] || fail "published archive omitted the documented packaged example"

export HOME="$run_root/home"
mkdir -p "$HOME"
export PATH="$install_root/bin:$PATH"
if [[ "$target" == *-apple-darwin || "$target" == x86_64-pc-windows-msvc ]]; then
  published_release_skip_linux_capabilities=1
fi
if [[ "$target" == x86_64-pc-windows-msvc ]]; then
  resolved_assay="$(command -v assay.exe)"
  resolved_mcp="$(command -v assay-mcp-server.exe)"
else
  resolved_assay="$(command -v assay)"
  resolved_mcp="$(command -v assay-mcp-server)"
fi
[[ "$resolved_assay" == "$assay_path" ]] || fail "assay did not resolve from the disposable install prefix"
[[ "$resolved_mcp" == "$mcp_path" ]] || fail "assay-mcp-server did not resolve from the disposable install prefix"

run_capture "assay-version" 0 "$results/assay-version.txt" "$results/assay-version.stderr" assay version
[[ "$(tr -d '\r\n' <"$results/assay-version.txt")" == "$version" ]] || fail "assay version differs from pinned release"
run_capture "mcp-version" 0 "$results/mcp-version.txt" "$results/mcp-version.stderr" assay-mcp-server --version
[[ "$(tr -d '\r\n' <"$results/mcp-version.txt")" == "assay-mcp-server $version" ]] \
  || fail "assay-mcp-server version differs from pinned release"

# Execute the public recipe itself before supplementary instrumented controls.
"$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" --documented-route
run_capture "verify-documented-default-profile" 2 "$results/documented-route/default-profile.json" "$results/documented-route/default-profile.stderr" \
  assay evidence verify-privileged-mcp-action "$results/documented-route/acquisition/project/action.bundle.tar.gz" --format json
"$JQ_BIN" -e '.bundle_integrity == "pass" and .verdict == "invalid" and .reason_code == "E_EVIDENCE_PROFILE_INVALID"' "$results/documented-route/default-profile.json" >/dev/null \
  || fail "documented produced denial bundle did not refuse incompatible default v0"

pushd "$session_root" >/dev/null
run_published_release_session_product
run_capture "policy-validate" 0 "$results/policy-validate.json" "$results/policy-validate.stderr" \
  assay policy validate --input policy.yaml --format json
"$JQ_BIN" -e '.schema == "assay.run_summary.v1"' "$results/policy-validate.json" >/dev/null || fail "policy validation output identity drifted"
run_capture "evaluate" 0 "$results/evaluate.json" "$results/evaluate.stderr" \
  assay run --config eval.yaml --trace-file traces/hello.jsonl --format json
"$JQ_BIN" -e '.schema == "assay.run_report.v1"' "$results/evaluate.json" >/dev/null || fail "evaluation output identity drifted"
popd >/dev/null

documented_init="$run_root/documented-init"
mkdir -p "$documented_init"
(
  cd "$documented_init"
  run_capture "init-documented" 0 "$results/init-documented.txt" "$results/init-documented.stderr" \
    assay init --preset dev --hello-trace
  run_published_release_doctor assay "$documented_init/eval.yaml"
)

decisions="$results/decisions.ndjson"
observations="$results/denied-observations.ndjson"
init_request='{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05","capabilities":{},"clientInfo":{"name":"published-release-gate","version":"1"}}}'
call_request='{"jsonrpc":"2.0","id":9,"method":"tools/call","params":{"name":"github.add_deploy_key","arguments":{"owner":"acme","repo":"prod-app"}}}'
proxy_status=0
printf '%s\n%s\n' "$init_request" "$call_request" \
  | (cd "$results" && \
      "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" \
        --timeout-seconds 60 --fixture-dir "$packaged_fixture" --expect deny) || proxy_status=$?
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
  assay evidence verify-privileged-mcp-action "$bundle" --format json --profile-version v1
"$JQ_BIN" -e '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "pass" and .verdict == "valid"' "$results/verify.json" >/dev/null \
  || fail "profile verification did not validate the produced bundle"
run_capture "verify-incompatible-default-profile" 2 \
  "$results/verify-default-profile.json" "$results/verify-default-profile.stderr" \
  assay evidence verify-privileged-mcp-action "$bundle" --format json
"$JQ_BIN" -e '.bundle_integrity == "pass" and .verdict == "invalid" and .reason_code == "E_EVIDENCE_PROFILE_INVALID"' "$results/verify-default-profile.json" >/dev/null \
  || fail "produced denial bundle did not refuse incompatible default v0"

preflight_offline_constructor

offline_assay="assay"
if [[ "$target" == x86_64-pc-windows-msvc ]]; then
  windows_harness_native="$(
    "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_offline_phase.py" \
      --print-harness-binary
  )"
  [[ -n "$windows_harness_native" ]] || fail "Windows offline harness path is empty"
  windows_harness_path="$windows_harness_native"
  if command -v cygpath >/dev/null 2>&1; then
    windows_harness_path="$(cygpath -u "$windows_harness_native")"
  fi
  mkdir -p "$(dirname "$windows_harness_path")"
  cp "$assay_path" "$windows_harness_path"
  "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" --windows-copy-before
  offline_assay="$windows_harness_native"
fi

# One helper classifies the loopback probe and runs this verifier under the target constructor.
offline_status=0
(cd "$results" && \
  "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_offline_phase.py" \
    --timeout-seconds 30 \
    -- \
    "${offline_assay:-assay}" evidence verify-privileged-mcp-action "$bundle" --profile-version v1 --format json) \
  || offline_status=$?
[[ "$offline_status" -eq 0 ]] || fail "offline isolation phase exited $offline_status"
cmp -s "$results/verify.json" "$results/verify-offline.json" \
  || fail "offline unshared verification output differs from connected verification"
# Apply the same offline constructor to the bundle made by the unchanged guide.
literal_results="$results/documented-route"
literal_bundle="$literal_results/acquisition/project/action.bundle.tar.gz"
"$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" --literal-input-before
run_capture "verify-documented-connected" 0 "$literal_results/verify.json" "$literal_results/verify.stderr" \
  assay evidence verify-privileged-mcp-action "$literal_bundle" --profile-version v1 --format json
"$JQ_BIN" -e '.bundle_integrity == "pass" and .verdict == "valid"' "$literal_results/verify.json" >/dev/null \
  || fail "documented connected verification did not validate the literal bundle"
documented_phase_status=0
(cd "$literal_results" && \
  "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_offline_phase.py" \
    --timeout-seconds 30 \
    -- \
    "${offline_assay:-assay}" evidence verify-privileged-mcp-action "$literal_bundle" --profile-version v1 --format json) \
  || documented_phase_status=$?
if [[ "$target" == x86_64-pc-windows-msvc ]]; then
  "$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" --windows-copy-after
fi
[[ "$documented_phase_status" -eq 0 ]] || fail "documented offline isolation phase exited $documented_phase_status"
cmp -s "$literal_results/verify.json" "$literal_results/verify-offline.json" \
  || fail "documented offline verification output differs from connected verification"
"$PYTHON_BIN" -I "$harness_root/scripts/ci/published_release_proxy_phase.py" --literal-input-after
if [[ "$target" == x86_64-pc-windows-msvc ]]; then
  "$PYTHON_BIN" - "$results/offline-operations.ndjson" "$results/offline-cleanup.json" \
    "$results/offline-claim-ceiling.txt" <<'PY'
import json, pathlib, sys
operations_path, cleanup_path, output_path = map(pathlib.Path, sys.argv[1:])
rows = [json.loads(line) for line in operations_path.read_text(encoding="utf-8").splitlines() if line]
matches = [row for row in rows if row.get("name") == "verify-produced-bundle-offline"]
if len(matches) != 1 or matches[0].get("classification") != "verified":
    raise SystemExit("Windows offline verifier operation is not uniquely verified")
claim = matches[0].get("claim_ceiling")
if not isinstance(claim, str) or not claim:
    raise SystemExit("Windows offline verifier operation omitted its claim ceiling")
cleanup = json.loads(cleanup_path.read_text(encoding="utf-8"))
if cleanup.get("status") != "clean":
    raise SystemExit("Windows offline cleanup is not clean")
output_path.write_text(claim + "\n", encoding="utf-8")
PY
fi

run_published_release_extra_request_cases

run_capture "export-sarif" 0 "$results/sarif.stdout" "$results/sarif.stderr" \
  assay-mcp-server enforcement-sarif --input "$decisions" --output "$results/enforcement.sarif"
"$JQ_BIN" -e '.version == "2.1.0" and (.runs[0].results | length == 1)' "$results/enforcement.sarif" >/dev/null \
  || fail "SARIF export identity or deny count drifted"
stdio_status=0
assay-mcp-server enforcement-sarif --input - --output - <"$decisions" >"$results/sarif-stdio.stdout" 2>"$results/sarif-stdio.stderr" || stdio_status=$?
record_command "export-sarif-stdio" "$stdio_status" \
  assay-mcp-server enforcement-sarif --input - --output -
[[ "$stdio_status" -eq 0 ]] || fail "documented SARIF stdio exited $stdio_status"
"$JQ_BIN" -e '.version == "2.1.0"' "$results/sarif-stdio.stdout" >/dev/null \
  || fail "documented SARIF stdio output identity drifted"

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
  assay evidence verify-privileged-mcp-action "$tampered" --format json --profile-version v1
"$JQ_BIN" -e '.schema == "assay.privileged_mcp_action.verify.report.v0" and .bundle_integrity == "fail" and .reason_code == "E_EVIDENCE_INTEGRITY"' "$results/tamper-verify.json" >/dev/null \
  || fail "tampered produced bundle did not fail with E_EVIDENCE_INTEGRITY"

if [[ "$target" != x86_64-pc-windows-msvc ]]; then
  [[ "$(sha256_file "$assay_path")" == "$("$JQ_BIN" -er '.reference_binary_sha256' "$results/installer/receipt.json")" ]] \
    || fail "installed CLI changed during the journey"
fi

driver_digest="$(sha256_file "$ROOT/scripts/ci/published-release-golden-path.sh")"
harness_manifest_digest="$(sha256_file "$HARNESS_MANIFEST")"
"$PYTHON_BIN" - "$release_tag" "$source_digest" "$results/attestation-summary.json" \
  "$harness_sha" "$workflow_run_id" "$workflow_run_attempt" "$driver_digest" \
  "$harness_manifest_digest" "$results/harness-files.json" "$commands_file" "$results/run-pin.json" <<'PY'
import json, pathlib, sys
(release_tag, source_digest, attestation_summary_path, harness_sha, workflow_run_id,
 workflow_run_attempt, driver_digest, harness_manifest_digest, harness_files_path,
 commands_path, output_path) = sys.argv[1:]
results_dir = pathlib.Path(commands_path).resolve().parent
target = (results_dir / "journey-target.txt").read_text(encoding="utf-8").strip()
platform_claim = (results_dir / "journey-platform-claim.txt").read_text(encoding="utf-8").strip()
if not target or not platform_claim:
    raise SystemExit("journey target/platform claim files are missing or empty")
attestations = json.loads(pathlib.Path(attestation_summary_path).read_text(encoding="utf-8"))
harness = json.loads(pathlib.Path(harness_files_path).read_text(encoding="utf-8"))
commands = [json.loads(line) for line in pathlib.Path(commands_path).read_text(encoding="utf-8").splitlines() if line]
document = {
    "schema": "assay.published_release_golden_path.run_pin.v1",
    "release": {
        "tag": release_tag,
        "source_ref": f"refs/tags/{release_tag}",
        "source_digest": source_digest,
        "target": target,
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
}
host = {}
for key, filename in (
    ("os", "host-uname-s.txt"),
    ("machine", "host-uname-m.txt"),
    ("proc_translated", "sysctl-proc-translated.txt"),
):
    host_path = results_dir / filename
    if host_path.is_file():
        host[key] = host_path.read_text(encoding="utf-8").strip()
if host:
    document["host"] = host
identity_path = results_dir / "checksum-consumer-identity.txt"
if identity_path.is_file():
    document["checksum_consumer_identity"] = identity_path.read_text(encoding="utf-8").strip()
documented = results_dir / "documented-route/receipt.json"
document["documented_route"] = json.loads(documented.read_text(encoding="utf-8"))
if document["documented_route"].get("status") != "completed":
    raise SystemExit("documented route proof is incomplete")
if target == "x86_64-pc-windows-msvc":
    continuity = json.loads((results_dir / "offline-binary-continuity.json").read_text())
    digests = [continuity[phase][key] for phase in ("before", "after")
               for key in ("source_sha256", "destination_sha256")]
    if len(set(digests)) != 1 or not all(isinstance(value, str) and len(value) == 64 for value in digests):
        raise SystemExit("Windows offline binary continuity is incomplete")
    document["offline_binary_continuity"] = continuity
installer = results_dir / "installer/receipt.json"
if installer.is_file():
    document["installer"] = json.loads(installer.read_text(encoding="utf-8"))
    if document["installer"].get("status") != "completed":
        raise SystemExit("installer proof is incomplete")
server_install = results_dir / "server-install.json"
if server_install.is_file():
    document["server_install"] = json.loads(server_install.read_text(encoding="utf-8"))
offline_claim_path = results_dir / "offline-claim-ceiling.txt"
if offline_claim_path.is_file():
    document["claim_ceiling"] = offline_claim_path.read_text(encoding="utf-8").strip()
else:
    document["claim_ceiling"] = (
        f"The attested release binaries completed the bounded {platform_claim} journey under the "
        "recorded harness head and fixture digests; the harness is not a shipped release asset. "
        "Doctor reports host capabilities, not kernel enforcement performed by this journey."
    )
pathlib.Path(output_path).write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

"$PYTHON_BIN" - "$results" "$results/retained-artifacts.json" <<'PY'
import hashlib, json, pathlib, sys
root, output = map(pathlib.Path, sys.argv[1:])
required = [
    "run-pin.json", "commands.ndjson", "doctor.json", "produced.bundle.tar.gz", "decisions.ndjson",
    "inspect.json", "verify.json", "tamper-verify.json", "enforcement.sarif",
    "release-api.json", "tag-ref.json", "attestation-summary.json",
    "allow/proxy.jsonl", "allow/decisions.ndjson", "allow/produced.bundle.tar.gz",
    "allow/verify.json", "unsupported/proxy.jsonl",
    "documented-route/receipt.json", "documented-route/acquisition/project/action.bundle.tar.gz",
    "documented-route/verify.json", "documented-route/verify-offline.json",
    "documented-route/offline-operations.ndjson", "documented-route/input-identity.json",
]
target = (root / "journey-target.txt").read_text(encoding="utf-8").strip()
archive_count = 1
if target != "x86_64-pc-windows-msvc":
    required.extend(["installer/receipt.json", "installer/default/executed-install.sh",
                     "installer/signed/executed-install.sh", "installer/contrasts.json"])
if target != "x86_64-pc-windows-msvc":
    required.append("server-install.json")
if target == "x86_64-pc-windows-msvc":
    required.extend([
        "verify-offline.json", "offline-operations.ndjson", "offline-cleanup.json",
        "offline-claim-ceiling.txt", "offline-binary-continuity.json",
        "documented-route/offline-cleanup.json",
    ])
for name in required:
    path = root / name
    if not path.is_file() or path.stat().st_size == 0:
        raise SystemExit(f"required retained artifact is missing or empty: {name}")
archives = list((root / "release-assets").glob("*.tar.gz")) + list((root / "release-assets").glob("*.zip"))
if len(archives) != archive_count or any(path.stat().st_size == 0 for path in archives):
    raise SystemExit("retained release archives are incomplete")
attestations = list((root / "attestation-raw").glob("*.json"))
if len(attestations) != archive_count or any(path.stat().st_size == 0 for path in attestations):
    raise SystemExit("retained attestation inputs are incomplete")
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
