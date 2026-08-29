#!/usr/bin/env bash
# jq programs intentionally use single quotes so shell variables are not expanded.
# shellcheck disable=SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
REPO="${GITHUB_REPOSITORY:-Rul1an/assay}"
GH_BIN="${GH_BIN:-gh}"
JQ_BIN="${JQ_BIN:-jq}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

usage() {
  echo "usage: published-release-historical-retention.sh --manifest <path> --harness-sha <40-hex> --workflow-run-id <id> --workflow-run-attempt <n> --run-root <abs-path> [--fixture-root <abs-path>]" >&2
  exit 2
}

manifest=""
harness_sha=""
run_root=""
workflow_run_id=""
workflow_run_attempt=""
fixture_root=""
while [[ "$#" -gt 0 ]]; do
  case "$1" in
    --manifest)
      [[ "$#" -ge 2 ]] || usage
      manifest="$2"
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
    --fixture-root)
      [[ "$#" -ge 2 ]] || usage
      fixture_root="$2"
      shift 2
      ;;
    *) usage ;;
  esac
done

[[ -n "$manifest" ]] || fail "manifest is required"
[[ "$manifest" = /* ]] || manifest="$ROOT/$manifest"
[[ -f "$manifest" ]] || fail "harness manifest is missing"
[[ "$harness_sha" =~ ^[0-9a-f]{40}$ ]] || fail "harness SHA must be exactly 40 lowercase hex characters"
[[ "$workflow_run_id" =~ ^[A-Za-z0-9_.-]+$ ]] || fail "workflow run id has an unsafe shape"
[[ "$workflow_run_attempt" =~ ^[0-9]+$ ]] || fail "workflow run attempt must be numeric"
[[ "$run_root" = /* ]] || fail "run root must be absolute"
[[ ! -e "$run_root" ]] || fail "run root already exists; refusing to reuse prior evidence: $run_root"
mapfile -t _release_pair < <(
  "$PYTHON_BIN" - "$manifest" <<'PY'
import json, pathlib, sys
manifest = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
from_tag = manifest["from_tag"]
to_tag = manifest["to_tag"]
if not isinstance(from_tag, str) or not from_tag.strip():
    raise SystemExit("harness manifest missing from_tag")
if not isinstance(to_tag, str) or not to_tag.strip():
    raise SystemExit("harness manifest missing to_tag")
print(from_tag)
print(to_tag)
PY
)
from_tag="${_release_pair[0]:-}"
to_tag="${_release_pair[1]:-}"
[[ -n "$from_tag" && -n "$to_tag" ]] || fail "harness manifest release pair is empty"
from_version="${from_tag#v}"
to_version="${to_tag#v}"
if [[ -n "$fixture_root" ]]; then
  [[ "$fixture_root" = /* ]] || fail "fixture root must be absolute"
fi

digest_file() {
  "$PYTHON_BIN" -c 'import hashlib,pathlib,sys; print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())' "$1"
}

install_root="$run_root/prefixes"
harness_root="$run_root/harness"
session_root="$run_root/session"
results="$run_root/results"
active_link="$run_root/active"
canary_path="$run_root/journey/.journey-canary"
ledger_file="$results/journey-ledger.ndjson"
commands_file="$results/commands.ndjson"
mkdir -p "$install_root" "$harness_root" "$session_root" "$results" "$run_root/home" "$run_root/journey"
: >"$commands_file"
: >"$ledger_file"

"$PYTHON_BIN" - "$manifest" "$ROOT" "$harness_root" "$results/harness-files.json" <<'PY'
import hashlib, json, pathlib, shutil, sys
manifest_path, root_path, output_path, report_path = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
if manifest.get("schema") != "assay.published_release_historical_retention.harness.v1":
    raise SystemExit("unexpected harness manifest schema")
report = []
for item in manifest["files"]:
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
report_path.write_text(json.dumps({"schema": manifest["schema"], "files": report}, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

record_stage() {
  local name="$1" dest_bin="$2"
  shift 2
  "$PYTHON_BIN" - "$commands_file" "$name" "$dest_bin" "$@" <<'PY'
import hashlib, json, pathlib, sys
path = pathlib.Path(sys.argv[1])
name, dest_bin = sys.argv[2], sys.argv[3]
record = {
    "name": name,
    "class": "state_producing",
    "exit_code": 0,
    "argv": sys.argv[4:],
    "subject_binary_sha256": hashlib.sha256(pathlib.Path(dest_bin).read_bytes()).hexdigest(),
}
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

record_activate() {
  local name="$1" status="$2" target="$3" subject="$4" reason="${5:-}"
  "$PYTHON_BIN" - "$commands_file" "$name" "$status" "$target" "$subject" "$reason" <<'PY'
import hashlib, json, pathlib, sys
path = pathlib.Path(sys.argv[1])
name, status, target, subject, reason = sys.argv[2:7]
record = {
    "name": name,
    "class": "activate",
    "exit_code": int(status),
    "activation_target": target,
    "subject_binary_sha256": hashlib.sha256(pathlib.Path(subject).read_bytes()).hexdigest(),
}
if reason:
    record["rejection_reason"] = reason
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

record_command() {
  local name="$1" class="$2" status="$3" stdout_path="$4" stderr_path="$5" flag_status="${6:-}"
  shift 6
  local bin="$1"
  "$PYTHON_BIN" - "$commands_file" "$name" "$class" "$status" "$stdout_path" "$stderr_path" "$flag_status" "$bin" "$@" <<'PY'
import hashlib, json, pathlib, sys
path = pathlib.Path(sys.argv[1])
name, class_name, status, stdout_path, stderr_path, flag_status, binary = sys.argv[2:9]
argv = sys.argv[9:]
def digest(value: str) -> str:
    return hashlib.sha256(pathlib.Path(value).read_bytes()).hexdigest() if value else hashlib.sha256(b"").hexdigest()
record = {
    "name": name,
    "class": class_name,
    "exit_code": int(status),
    "argv": argv,
    "stdout_sha256": digest(stdout_path),
    "stderr_sha256": digest(stderr_path),
    "executed_binary_sha256": digest(binary),
    "selected_profile": "v1" if "--profile-version" in argv and "v1" in argv else "v0",
}
if flag_status:
    record["flag_status"] = flag_status
with path.open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(record, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

run_capture() {
  local name="$1" class="$2" expected="$3" stdout_path="$4" stderr_path="$5"
  shift 5
  local status=0
  "$@" >"$stdout_path" 2>"$stderr_path" || status=$?
  record_command "$name" "$class" "$status" "$stdout_path" "$stderr_path" "" "$@"
  [[ "$status" -eq "$expected" ]] || fail "$name exited $status, expected $expected"
}

write_ledger() {
  local boundary="$1"
  "$PYTHON_BIN" - "$ledger_file" "$boundary" "$active_link" "$canary_path" "$run_root" "$results" "$manifest" <<'PY'
import hashlib, json, os, pathlib, sys
ledger, boundary, active, canary, run_root, results, manifest_path = sys.argv[1:]
active_path = pathlib.Path(active).resolve()
run_root_path = pathlib.Path(run_root)
manifest = json.loads(pathlib.Path(manifest_path).read_text(encoding="utf-8"))
relatives = manifest["required_retained_artifacts"]
if not isinstance(relatives, list) or not relatives:
    raise SystemExit("harness manifest missing required_retained_artifacts")
files = []
for relative in relatives:
    path = run_root_path.joinpath(*pathlib.PurePosixPath(relative).parts)
    if not path.is_file():
        continue
    stat = path.stat()
    files.append({
        "path": path.relative_to(run_root_path).as_posix(),
        "sha256": hashlib.sha256(path.read_bytes()).hexdigest(),
        "inode": stat.st_ino,
        "birth_time": getattr(stat, "st_birthtime", None),
    })
row = {
    "boundary": boundary,
    "activation_target": active_path.name,
    "canary_sha256": hashlib.sha256(pathlib.Path(canary).read_bytes()).hexdigest(),
    "active_binary_sha256": hashlib.sha256((active_path / "bin/assay").read_bytes()).hexdigest(),
    "files": files,
}
with pathlib.Path(ledger).open("a", encoding="utf-8") as handle:
    handle.write(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n")
PY
}

activate_prefix() {
  local prefix="$1" expected_version="$2" expected_digest="$3"
  local bin="$prefix/bin/assay"
  local actual
  actual="$(digest_file "$bin")"
  [[ "$actual" == "$expected_digest" ]] || {
    echo "prefix digest mismatch before activation: $prefix" >&2
    return 1
  }
  local ver
  ver="$("$bin" version | tr -d '\r\n')"
  [[ "$ver" == "$expected_version" ]] || {
    echo "prefix version mismatch before activation: $prefix" >&2
    return 1
  }
  ln -sfn "$prefix" "$active_link"
}

stage_fixture_prefix() {
  local tag="$1"
  local dest="$install_root/$tag"
  mkdir -p "$dest"
  cp -R "$fixture_root/$tag/." "$dest/"
  chmod 0755 "$dest/bin/assay"
  record_stage "stage-prefix-$tag" "$dest/bin/assay" cp -R "$fixture_root/$tag/." "$dest/"
}

stage_published_prefix() {
  local tag="$1"
  local downloads="$results/release-assets/$tag"
  mkdir -p "$downloads"
  local release_api="$results/release-api-$tag.json"
  "$GH_BIN" api "repos/${REPO}/releases/tags/${tag}" >"$release_api"
  "$JQ_BIN" -e '.draft == false and .prerelease == false' "$release_api" >/dev/null \
    || fail "release tag is still draft or prerelease"
  local cli_asset="assay-${tag}-x86_64-unknown-linux-gnu.tar.gz"
  local mcp_asset="assay-mcp-server-${tag}-x86_64-unknown-linux-gnu.tar.gz"
  PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
    'import pathlib,sys; from bounded_download import download; download(sys.argv[1], pathlib.Path(sys.argv[2]), max_bytes=int(sys.argv[3]))' \
    "https://github.com/${REPO}/releases/download/${tag}/${cli_asset}" "$downloads/$cli_asset" 67108864
  PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
    'import pathlib,sys; from bounded_download import download; download(sys.argv[1], pathlib.Path(sys.argv[2]), max_bytes=int(sys.argv[3]))' \
    "https://github.com/${REPO}/releases/download/${tag}/${mcp_asset}" "$downloads/$mcp_asset" 33554432
  local tag_ref="$results/tag-ref-$tag.json"
  "$GH_BIN" api "repos/${REPO}/git/ref/tags/${tag}" >"$tag_ref"
  local source_type source_digest
  source_type="$($JQ_BIN -er '.object.type' "$tag_ref")"
  source_digest="$($JQ_BIN -er '.object.sha' "$tag_ref")"
  if [[ "$source_type" == "tag" ]]; then
    "$GH_BIN" api "repos/${REPO}/git/tags/${source_digest}" >"$results/tag-object-$tag.json"
    source_digest="$($JQ_BIN -er '.object.sha' "$results/tag-object-$tag.json")"
  fi
  local signer_workflow="$REPO/.github/workflows/release.yml"
  if ! GH_BIN="$GH_BIN" JQ_BIN="$JQ_BIN" \
    ASSETS_DIR="$downloads" \
    OUT_SUMMARY="$results/attestation-summary-$tag.json" \
    OUT_RAW_DIR="$results/attestation-raw-$tag" \
    REPO="$REPO" \
    SIGNER_WORKFLOW="$signer_workflow" \
    SOURCE_REF="" \
    SOURCE_DIGEST="$source_digest" \
    bash "$harness_root/scripts/ci/release_attestation_enforce.sh" \
    >"$results/attestation-verify-$tag.log" 2>&1; then
    cat "$results/attestation-verify-$tag.log" >&2
    fail "reviewed release attestation verifier rejected the published assets"
  fi
  local extract="$run_root/extract-$tag"
  PYTHONPATH="$harness_root/scripts/ci" "$PYTHON_BIN" -c \
    'import pathlib,sys; from safe_extract_release_archive import extract_archive; extract_archive(pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2]), max_decoded_bytes=int(sys.argv[3]))' \
    "$downloads/$cli_asset" "$extract" 134217728
  mkdir -p "$install_root/$tag/bin"
  mapfile -t cli_candidates < <(find "$extract" -type f -name assay -perm -u+x)
  [[ "${#cli_candidates[@]}" -eq 1 ]] || fail "CLI archive must contain exactly one executable assay binary"
  cp "${cli_candidates[0]}" "$install_root/$tag/bin/assay"
  chmod 0755 "$install_root/$tag/bin/assay"
  record_stage "stage-prefix-$tag" "$install_root/$tag/bin/assay" cp "${cli_candidates[0]}" "$install_root/$tag/bin/assay"
}

if [[ -n "$fixture_root" ]]; then
  stage_fixture_prefix "$from_tag"
  stage_fixture_prefix "$to_tag"
else
  stage_published_prefix "$from_tag"
  stage_published_prefix "$to_tag"
fi

digest_530="$(digest_file "$install_root/$from_tag/bin/assay")"
digest_540="$(digest_file "$install_root/$to_tag/bin/assay")"
activate_prefix "$install_root/$from_tag" "$from_version" "$digest_530" || fail "v5.3 activation failed"

export HOME="$run_root/home"
export PATH="$active_link/bin:/usr/bin:/bin"

canary_python="$(command -v "$PYTHON_BIN")"
[[ -n "$canary_python" ]] || fail "python interpreter is missing"
"$canary_python" -c 'import os,pathlib,sys; pathlib.Path(sys.argv[1]).write_bytes(os.urandom(32))' "$canary_path"
record_command "create-journey-canary" "state_producing" 0 /dev/null /dev/null "" \
  "$canary_python" -c 'import os,pathlib,sys; pathlib.Path(sys.argv[1]).write_bytes(os.urandom(32))' "$canary_path"

pushd "$session_root" >/dev/null
run_capture "init" "state_producing" 0 "$results/init.json" "$results/init.stderr" \
  "$active_link/bin/assay" init --preset dev --hello-trace --format json
run_capture "migrate-check-v5.3" "observe" 0 "$results/migrate-v53.txt" "$results/migrate-v53.stderr" \
  "$active_link/bin/assay" migrate --check --config eval.yaml
run_capture "export-v0-bundle" "state_producing" 0 "$results/export-v0.stdout" "$results/export-v0.stderr" \
  "$active_link/bin/assay" evidence export --profile eval.yaml -o "$results/v0.bundle"
explicit_status=0
"$active_link/bin/assay" evidence verify-privileged-mcp-action "$results/v0.bundle" --format json --profile-version v1 \
  >"$results/explicit-v1.json" 2>"$results/explicit-v1.stderr" || explicit_status=$?
record_command "explicit-v1-v5.3" "observe" "$explicit_status" "$results/explicit-v1.json" "$results/explicit-v1.stderr" "flag_unavailable" \
  "$active_link/bin/assay" evidence verify-privileged-mcp-action "$results/v0.bundle" --format json --profile-version v1
[[ "$explicit_status" -ne 0 ]] || fail "v5.3 explicit v1 must be unavailable"
popd >/dev/null
write_ledger "v5.3-created"

corrupt="$run_root/corrupt-$to_tag"
mkdir -p "$corrupt/bin"
cp "$install_root/$to_tag/bin/assay" "$corrupt/bin/assay"
"$PYTHON_BIN" - "$corrupt/bin/assay" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
data = bytearray(path.read_bytes())
data[-1] = data[-1] ^ 0x01
path.write_bytes(bytes(data))
PY
failed_status=0
activate_prefix "$corrupt" "5.4.0" "$digest_540" \
  >"$results/failed-activate.stdout" 2>"$results/failed-activate.stderr" || failed_status=$?
[[ "$failed_status" -ne 0 ]] || fail "corrupt v5.4 activation must fail"
record_activate "failed-activate-v5.4" "$failed_status" "$to_tag" "$corrupt/bin/assay" \
  "$(tr -d '\r\n' <"$results/failed-activate.stderr")"
run_capture "post-failed-activation-active" "observe" 0 "$results/post-fail-version.txt" "$results/post-fail-version.stderr" \
  "$active_link/bin/assay" version
[[ "$(tr -d '\r\n' <"$results/post-fail-version.txt")" == "$from_version" ]] || fail "active version after failed activation must remain $from_version"
write_ledger "failed-v5.4-activation"

activate_prefix "$install_root/$to_tag" "$to_version" "$digest_540" || fail "v5.4 activation failed"
record_activate "activate-v5.4" 0 "$to_tag" "$install_root/$to_tag/bin/assay"
run_capture "assay-version-v5.4" "observe" 0 "$results/assay-version-v54.txt" "$results/assay-version-v54.stderr" \
  "$active_link/bin/assay" version
[[ "$(tr -d '\r\n' <"$results/assay-version-v54.txt")" == "$to_version" ]] || fail "active version after v5.4 activation must be $to_version"
write_ledger "v5.4-activated"

pushd "$session_root" >/dev/null
run_capture "migrate-check-v5.4" "observe" 0 "$results/migrate-v54.txt" "$results/migrate-v54.stderr" \
  "$active_link/bin/assay" migrate --check --config eval.yaml
v0_status=0
"$active_link/bin/assay" evidence verify "$results/v0.bundle" \
  >"$results/verify-v0-v54.json" 2>"$results/verify-v0-v54.stderr" || v0_status=$?
record_command "verify-v0-under-v5.4" "observe" "$v0_status" "$results/verify-v0-v54.json" "$results/verify-v0-v54.stderr" "" \
  "$active_link/bin/assay" evidence verify "$results/v0.bundle"
printf '%s\n' '{"schema":"assay.enforcement_decision.v0","caller":{"id":"historical"},"tool":{"name":"github.add_deploy_key","action_class":"github_deploy_key"},"action":{"verb":"create","resource_type":"github_deploy_key","target":null,"target_digest":null},"decision":"deny","reason":"no_declared_allowance","fail_closed":true,"drift_state":"not_evaluated","credential_alias":"gh-deploy","non_claims":[]}' >"$results/decisions.ndjson"
run_capture "import-v1-bundle" "state_producing" 0 "$results/import-v1.stdout" "$results/import-v1.stderr" \
  "$active_link/bin/assay" evidence import privileged-mcp-action --decisions "$results/decisions.ndjson" --bundle-out "$results/v1.bundle"
run_capture "verify-v1-under-v5.4" "observe" 0 "$results/verify-v1.json" "$results/verify-v1.stderr" \
  "$active_link/bin/assay" evidence verify-privileged-mcp-action "$results/v1.bundle" --format json --profile-version v1
popd >/dev/null
write_ledger "v5.4-v0-verified-v1-created"

activate_prefix "$install_root/$from_tag" "$from_version" "$digest_530" || fail "v5.3 reactivation failed"
record_activate "reactivate-v5.3" 0 "$from_tag" "$install_root/$from_tag/bin/assay"
run_capture "post-reactivation-active" "observe" 0 "$results/reactivate-version.txt" "$results/reactivate-version.stderr" \
  "$active_link/bin/assay" version
[[ "$(tr -d '\r\n' <"$results/reactivate-version.txt")" == "$from_version" ]] || fail "reactivated active version must be $from_version"
write_ledger "v5.3-reactivated"

driver_digest="$(digest_file "$ROOT/scripts/ci/published-release-historical-retention.sh")"
"$PYTHON_BIN" - "$from_tag" "$to_tag" "$harness_sha" "$workflow_run_id" "$workflow_run_attempt" \
  "$driver_digest" "$v0_status" "$results/run-pin.json" <<'PY'
import json, os, pathlib, sys
(from_tag, to_tag, harness_sha, workflow_run_id, workflow_run_attempt, driver_digest, v0_status, output) = sys.argv[1:]
document = {
    "schema": "assay.published_release_historical_retention.run_pin.v1",
    "from_tag": from_tag,
    "to_tag": to_tag,
    "harness": {
        "head_sha": harness_sha,
        "workflow_run_id": workflow_run_id,
        "workflow_run_attempt": int(workflow_run_attempt),
        "driver_sha256": driver_digest,
    },
    "migration_required": False,
    "v0_cross_version_verify": {
        "disposition": "unmeasured",
        "recorded_exit": int(v0_status),
    },
    "image_os": os.environ.get("IMAGE_OS", ""),
    "image_version": os.environ.get("IMAGE_VERSION", ""),
    "runner_os": os.environ.get("RUNNER_OS", ""),
    "runner_arch": os.environ.get("RUNNER_ARCH", ""),
    "claim_ceiling": (
        "the harness observed single-creation byte continuity for the instrumented "
        f"Linux x86_64 published pair {from_tag} -> {to_tag} -> retained {from_tag}. The claim is "
        "limited to recorded harness operations. Hosted published-binary outcomes remain "
        "a separate dispatch. A byte-identical copy-aside/restore is outside the claim. "
        "Cross-version v0 verify remains unmeasured pending hosted dispatch."
    ),
}
pathlib.Path(output).write_text(json.dumps(document, indent=2, sort_keys=True) + "\n", encoding="utf-8")
PY

echo "PASS: historical retention harness recorded $from_tag -> $to_tag -> retained $from_tag"
