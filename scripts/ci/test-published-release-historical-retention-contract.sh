#!/usr/bin/env bash
# Mutation anchors intentionally preserve literal shell and Actions expressions.
# shellcheck disable=SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/published-release-historical-retention.yml}"
DRIVER="${DRIVER:-${ROOT}/scripts/ci/published-release-historical-retention.sh}"
MANIFEST="${MANIFEST:-${ROOT}/scripts/ci/fixtures/published-release-historical-retention/v1/harness-manifest.json}"
CHECKER="${ROOT}/scripts/ci/check-published-release-historical-retention-contract.py"
EXPECTED_HEAD_SHA="${EXPECTED_HEAD_SHA:-6704d0bc4029f893f9558ab669ffc60918971943}"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

[[ -f "$CHECKER" ]] || fail "missing historical-retention checker"
[[ -f "$WORKFLOW" ]] || fail "missing historical-retention workflow"
[[ -f "$DRIVER" ]] || fail "missing historical-retention driver"
[[ -f "$MANIFEST" ]] || fail "missing historical-retention harness manifest"

python3 "$CHECKER" \
  --workflow "$WORKFLOW" \
  --driver "$DRIVER" \
  --manifest "$MANIFEST" \
  --source-root "$ROOT"

scratch="$(mktemp -d)"
trap 'chmod -R u+w "$scratch" 2>/dev/null || true; rm -rf "$scratch"' EXIT

write_stub_assay() {
  local dest="$1" version="$2"
  mkdir -p "$(dirname "$dest")"
  cat >"$dest" <<STUB
#!/usr/bin/env bash
set -euo pipefail
VERSION="$version"
cmd="\${1:-}"
shift || true
if [[ "\$*" == *"--profile-version"* && "\$VERSION" == "5.3.0" ]]; then
  echo "error: unexpected argument '--profile-version' found" >&2
  exit 2
fi
case "\$cmd" in
  version)
    printf '%s\\n' "\$VERSION"
    ;;
  init)
    mkdir -p "\$HOME/.config/assay" traces
    printf 'home-config\\n' >"\$HOME/.config/assay/config.toml"
    printf 'policy\\n' >policy.yaml
    printf 'config\\n' >eval.yaml
    printf 'trace\\n' >traces/hello.jsonl
    printf '%s\\n' '{"schema":"assay.init_report.v0"}'
    ;;
  migrate)
    echo "Config eval.yaml is clean (already migrated)."
    exit 0
    ;;
  evidence)
    sub="\${1:-}"
    shift || true
    out=""
    prev=""
    for arg in "\$@"; do
      if [[ "\$prev" == "-o" || "\$prev" == "--out" || "\$prev" == "--bundle-out" ]]; then
        out="\$arg"
      fi
      prev="\$arg"
    done
    case "\$sub" in
      export)
        [[ -n "\$out" ]] || exit 2
        printf 'v0-bundle\\n' >"\$out"
        ;;
      verify)
        exit 0
        ;;
      import)
        [[ -n "\$out" ]] || exit 2
        printf 'v1-bundle\\n' >"\$out"
        ;;
      verify-privileged-mcp-action)
        exit 0
        ;;
      *)
        exit 2
        ;;
    esac
    ;;
  *)
    exit 2
    ;;
esac
STUB
  chmod 0755 "$dest"
}

build_fixture_root() {
  local fixture="$1"
  write_stub_assay "$fixture/v5.3.0/bin/assay" "5.3.0"
  write_stub_assay "$fixture/v5.4.0/bin/assay" "5.4.0"
}

run_driver() {
  local run_root="$1" fixture="$2"
  IMAGE_OS=ubuntu24 IMAGE_VERSION=20260824.1.0 RUNNER_OS=Linux RUNNER_ARCH=X64 \
  bash "$DRIVER" \
    --manifest "$MANIFEST" \
    --harness-sha "$EXPECTED_HEAD_SHA" \
    --workflow-run-id test-historical \
    --workflow-run-attempt 1 \
    --run-root "$run_root" \
    --fixture-root "$fixture"
}

hosted_consumer_check() {
  # Must stay identical to the hosted workflow consumer-checker argv.
  python3 "$CHECKER" \
    --results "$1" \
    --manifest "$MANIFEST" \
    --expected-head-sha "$EXPECTED_HEAD_SHA"
}

check_results() {
  hosted_consumer_check "$1/results"
}

expect_results_failure() {
  local name="$1" results="$2" expected="$3"
  expect_results_failure_with_manifest "$name" "$results" "$MANIFEST" "$expected"
}

expect_results_failure_with_manifest() {
  local name="$1" results="$2" manifest="$3" expected="$4"
  if python3 "$CHECKER" --results "$results" --manifest "$manifest" \
      --expected-head-sha "$EXPECTED_HEAD_SHA" >"$scratch/$name.out" 2>&1; then
    fail "results mutation stayed green: $name"
  fi
  grep -F "$expected" "$scratch/$name.out" >/dev/null \
    || fail "results mutation $name missed expected guard: $expected"
}

expect_source_failure() {
  local name="$1" target="$2" old="$3" new="$4" expected="$5"
  local case_root="$scratch/$name"
  mkdir -p "$case_root"
  cp "$WORKFLOW" "$case_root/workflow.yml"
  cp "$DRIVER" "$case_root/driver.sh"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$case_root/$target" "$old" "$new" <<'PY'
import pathlib, sys
path = pathlib.Path(sys.argv[1])
old, new = sys.argv[2:]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor count for {old!r}: {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
  if [[ "$target" == "manifest.json" || "$target" == "driver.sh" || "$target" == "workflow.yml" ]]; then
    python3 - "$case_root/manifest.json" "$case_root/workflow.yml" "$case_root/driver.sh" <<'PY'
import hashlib, json, pathlib, sys
manifest_path, workflow, driver = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
refresh = {
    ".github/workflows/published-release-historical-retention.yml": workflow,
    "scripts/ci/published-release-historical-retention.sh": driver,
}
for row in manifest["files"]:
    if row["path"] in refresh:
        row["sha256"] = hashlib.sha256(refresh[row["path"]].read_bytes()).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  fi
  if python3 "$CHECKER" \
      --workflow "$case_root/workflow.yml" \
      --driver "$case_root/driver.sh" \
      --manifest "$case_root/manifest.json" \
      --source-root "$ROOT" >"$case_root/output" 2>&1; then
    fail "source mutation stayed green: $name"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "source mutation $name missed expected guard: $expected"
}

write_mutated_manifest() {
  local dest="$1"
  python3 - "$MANIFEST" "$dest" "$2" "$3" <<'PY'
import json, pathlib, sys
src, dest = map(pathlib.Path, sys.argv[1:3])
kind, value = sys.argv[3], sys.argv[4]
manifest = json.loads(src.read_text(encoding="utf-8"))
if kind == "from_tag":
    manifest["from_tag"] = value
elif kind == "to_tag":
    manifest["to_tag"] = value
elif kind == "artifact":
    artifacts = list(manifest["required_retained_artifacts"])
    artifacts[0] = value
    manifest["required_retained_artifacts"] = artifacts
elif kind == "duplicate_artifact":
    artifacts = list(manifest["required_retained_artifacts"])
    artifacts.insert(0, artifacts[0])
    manifest["required_retained_artifacts"] = artifacts
elif kind == "release_pair":
    from_tag, to_tag = value.split(",", 1)
    manifest["from_tag"] = from_tag
    manifest["to_tag"] = to_tag
else:
    raise SystemExit(f"unknown manifest mutation {kind}")
dest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
}

expect_driver_unsafe_manifest() {
  local name="$1" kind="$2" value="$3" expected="$4"
  local case_root="$scratch/$name"
  local run_root="$case_root/run"
  local mutated="$case_root/manifest.json"
  mkdir -p "$case_root"
  write_mutated_manifest "$mutated" "$kind" "$value"
  if IMAGE_OS=ubuntu24 IMAGE_VERSION=20260824.1.0 RUNNER_OS=Linux RUNNER_ARCH=X64 \
    bash "$DRIVER" \
      --manifest "$mutated" \
      --harness-sha "$EXPECTED_HEAD_SHA" \
      --workflow-run-id test-historical \
      --workflow-run-attempt 1 \
      --run-root "$run_root" \
      --fixture-root "$fixture" >"$case_root/driver.out" 2>&1; then
    fail "driver mutation stayed green: $name"
  fi
  grep -F "$expected" "$case_root/driver.out" >/dev/null \
    || fail "driver mutation $name missed expected guard: $expected"
  if [[ -e "$run_root/results/journey-ledger.ndjson" ]]; then
    if [[ -s "$run_root/results/journey-ledger.ndjson" ]]; then
      fail "driver mutation $name wrote a ledger row"
    fi
  fi
  if [[ -e "$run_root" ]]; then
    fail "driver mutation $name materialized run_root"
  fi
}

expect_path_safety_parity() {
  local name="$1" kind="$2" value="$3" expected="$4"
  local case_root="$scratch/parity-$name"
  local mutated="$case_root/manifest.json"
  mkdir -p "$case_root"
  write_mutated_manifest "$mutated" "$kind" "$value"
  if python3 "$CHECKER" \
      --workflow "$WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$mutated" \
      --source-root "$ROOT" >"$case_root/checker.out" 2>&1; then
    fail "parity checker stayed green: $name"
  fi
  grep -F "$expected" "$case_root/checker.out" >/dev/null \
    || fail "parity checker $name missed expected guard: $expected"
  expect_driver_unsafe_manifest "parity-driver-$name" "$kind" "$value" "$expected"
}

expect_precommit_helper_decoy() {
  local helper_path="$1"
  local case_root="$scratch/hook-decoy-${helper_path//\//_}"
  local expected="historical-retention pre-commit files selector must match '${helper_path}'"
  mkdir -p "$case_root"
  cp "$ROOT/.pre-commit-config.yaml" "$case_root/pre-commit.yaml"
  python3 - "$case_root/pre-commit.yaml" "$helper_path" <<'PY'
import pathlib, re, sys
path, helper = pathlib.Path(sys.argv[1]), sys.argv[2]
escaped = re.escape(pathlib.Path(helper).name)
text = path.read_text(encoding="utf-8")
lines = text.splitlines(keepends=True)
in_hook = False
for index, line in enumerate(lines):
    if re.match(r"^\s+- id: published-release-historical-retention-contract\s*$", line):
        in_hook = True
        continue
    if in_hook and re.match(r"^\s+- id:", line):
        break
    if not in_hook:
        continue
    match = re.match(r"^(\s+files:\s+)(\S+)\s*$", line)
    if not match:
        continue
    prefix, selector = match.group(1), match.group(2)
    updated = selector.replace(f"|{escaped}", "", 1)
    if updated == selector:
        updated = selector.replace(f"{escaped}|", "", 1)
    if updated == selector:
        raise SystemExit(f"historical-retention files: regex does not contain {escaped}")
    try:
        compiled = re.compile(updated)
    except re.error as error:
        raise SystemExit(f"mutated files: selector is invalid: {error}") from error
    if compiled.search(helper) is not None:
        raise SystemExit(f"decoy setup still matches {helper}")
    indent = re.match(r"^(\s*)", line).group(1)
    lines[index] = f"{prefix}{updated}\n"
    lines.insert(index + 1, f"{indent}# {helper}\n")
    path.write_text("".join(lines), encoding="utf-8")
    raise SystemExit
raise SystemExit("historical-retention files: selector not found")
PY
  if python3 "$CHECKER" \
      --workflow "$WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$MANIFEST" \
      --source-root "$ROOT" \
      --pre-commit "$case_root/pre-commit.yaml" >"$case_root/output" 2>&1; then
    fail "source mutation stayed green: hook-decoy $helper_path"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "hook-decoy $helper_path missed expected guard: $expected"
}

expect_materialized_helper_modes() {
  local helper="$good_root/harness/scripts/ci/release_archive_inventory.sh"
  local control="$good_root/harness/scripts/ci/bounded_download.py"
  local report="$good_root/results/harness-files.json"
  [[ -f "$helper" ]] || fail "materialized inventory helper is missing"
  [[ -f "$control" ]] || fail "materialized non-executable control is missing"
  [[ -f "$report" ]] || fail "harness-files.json is missing"
  [[ -x "$helper" ]] || fail "declared executable helper materialized non-executable"
  [[ ! -x "$control" ]] || fail "non-executable control became executable"
  python3 - "$report" <<'PY'
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
files = report.get("files")
if not isinstance(files, list):
    raise SystemExit("harness-files.json has no files")
by_path = {row.get("path"): row for row in files if isinstance(row, dict)}
helper = by_path.get("scripts/ci/release_archive_inventory.sh")
control = by_path.get("scripts/ci/bounded_download.py")
if not helper or helper.get("executable") is not True:
    raise SystemExit("inventory helper executable report drifted")
if not control or control.get("executable") is not False:
    raise SystemExit("non-executable control executable report drifted")
PY
}

expect_driver_neutralized_chmod() {
  local case_root="$scratch/neutralized-chmod"
  local run_root="$case_root/run"
  local repo_root="$case_root/repo"
  local expected="harness executable observation drifted"
  mkdir -p "$repo_root/scripts/ci" "$repo_root/.github/workflows"
  ln -s "$ROOT/.github/workflows/published-release-historical-retention.yml" \
    "$repo_root/.github/workflows/published-release-historical-retention.yml"
  ln -s "$ROOT/scripts/ci/release_attestation_enforce.sh" "$repo_root/scripts/ci/release_attestation_enforce.sh"
  ln -s "$ROOT/scripts/ci/release_archive_inventory.sh" "$repo_root/scripts/ci/release_archive_inventory.sh"
  ln -s "$ROOT/scripts/ci/safe_extract_release_archive.py" "$repo_root/scripts/ci/safe_extract_release_archive.py"
  ln -s "$ROOT/scripts/ci/bounded_download.py" "$repo_root/scripts/ci/bounded_download.py"
  cp "$DRIVER" "$repo_root/scripts/ci/published-release-historical-retention.sh"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$repo_root/scripts/ci/published-release-historical-retention.sh" "$case_root/manifest.json" <<'PY'
import hashlib, json, pathlib, sys
driver, manifest_path = map(pathlib.Path, sys.argv[1:])
text = driver.read_text(encoding="utf-8")
old = "    if executable:\n"
new = "    if executable and False:\n"
if text.count(old) != 1:
    raise SystemExit(f"chmod guard count: {text.count(old)}")
driver.write_text(text.replace(old, new, 1), encoding="utf-8")
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
digest = hashlib.sha256(driver.read_bytes()).hexdigest()
found = False
for row in manifest.get("files") or []:
    if row.get("path") == "scripts/ci/published-release-historical-retention.sh":
        row["sha256"] = digest
        found = True
if not found:
    raise SystemExit("driver row missing from manifest")
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  if IMAGE_OS=ubuntu24 IMAGE_VERSION=20260824.1.0 RUNNER_OS=Linux RUNNER_ARCH=X64 \
    bash "$repo_root/scripts/ci/published-release-historical-retention.sh" \
      --manifest "$case_root/manifest.json" \
      --harness-sha "$EXPECTED_HEAD_SHA" \
      --workflow-run-id test-historical \
      --workflow-run-attempt 1 \
      --run-root "$run_root" \
      --fixture-root "$fixture" >"$case_root/driver.out" 2>&1; then
    fail "driver mutation stayed green: neutralized-chmod"
  fi
  grep -F "$expected" "$case_root/driver.out" >/dev/null \
    || fail "neutralized-chmod missed expected driver guard: $expected"
  if [[ -f "$run_root/results/harness-files.json" ]]; then
    python3 - "$run_root/results/harness-files.json" <<'PY' || fail "neutralized-chmod wrote a false executable report"
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
for row in report.get("files") or []:
    if row.get("path") == "scripts/ci/release_archive_inventory.sh" and row.get("executable") is True:
        raise SystemExit("retained report repeated the declaration")
PY
  fi
}


expect_driver_report_digest_lie() {
  local case_root="$scratch/report-digest-lie"
  local run_root="$case_root/run"
  local repo_root="$case_root/repo"
  local expected="harness files observation drifted"
  mkdir -p "$repo_root/scripts/ci" "$repo_root/.github/workflows"
  ln -s "$ROOT/.github/workflows/published-release-historical-retention.yml" \
    "$repo_root/.github/workflows/published-release-historical-retention.yml"
  ln -s "$ROOT/scripts/ci/release_attestation_enforce.sh" "$repo_root/scripts/ci/release_attestation_enforce.sh"
  ln -s "$ROOT/scripts/ci/release_archive_inventory.sh" "$repo_root/scripts/ci/release_archive_inventory.sh"
  ln -s "$ROOT/scripts/ci/safe_extract_release_archive.py" "$repo_root/scripts/ci/safe_extract_release_archive.py"
  ln -s "$ROOT/scripts/ci/bounded_download.py" "$repo_root/scripts/ci/bounded_download.py"
  cp "$DRIVER" "$repo_root/scripts/ci/published-release-historical-retention.sh"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$repo_root/scripts/ci/published-release-historical-retention.sh" "$case_root/manifest.json" <<'PY'
import hashlib, json, pathlib, sys
driver, manifest_path = map(pathlib.Path, sys.argv[1:])
text = driver.read_text(encoding="utf-8")
old = '"sha256": digest'
new = '"sha256": "0000000000000000000000000000000000000000000000000000000000000000"'
if text.count(old) != 1:
    raise SystemExit(f"report digest field count: {text.count(old)}")
driver.write_text(text.replace(old, new, 1), encoding="utf-8")
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
digest = hashlib.sha256(driver.read_bytes()).hexdigest()
found = False
for row in manifest.get("files") or []:
    if row.get("path") == "scripts/ci/published-release-historical-retention.sh":
        row["sha256"] = digest
        found = True
if not found:
    raise SystemExit("driver row missing from manifest")
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  local driver_rc=0
  IMAGE_OS=ubuntu24 IMAGE_VERSION=20260824.1.0 RUNNER_OS=Linux RUNNER_ARCH=X64 \
    bash "$repo_root/scripts/ci/published-release-historical-retention.sh" \
      --manifest "$case_root/manifest.json" \
      --harness-sha "$EXPECTED_HEAD_SHA" \
      --workflow-run-id test-historical \
      --workflow-run-attempt 1 \
      --run-root "$run_root" \
      --fixture-root "$fixture" >"$case_root/driver.out" 2>&1 || driver_rc=$?
  local checker_rc=1
  if [[ -f "$run_root/results/harness-files.json" ]]; then
    python3 - "$run_root/results/harness-files.json" <<'PY' || fail "report-digest-lie did not write the zeroed report digest"
import json, pathlib, sys
report = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
helper = "scripts/ci/release_archive_inventory.sh"
found = False
for row in report.get("files") or []:
    if row.get("path") == helper:
        found = True
        if row.get("sha256") != "0" * 64:
            raise SystemExit("retained report kept the observed digest")
if not found:
    raise SystemExit("retained report missing inventory helper")
PY
    checker_rc=0
    python3 "$CHECKER" \
      --results "$run_root/results" \
      --manifest "$case_root/manifest.json" \
      --expected-head-sha "$EXPECTED_HEAD_SHA" >"$case_root/checker.out" 2>&1 || checker_rc=$?
  fi
  if [[ "$driver_rc" -eq 0 && "$checker_rc" -eq 0 ]]; then
    fail "driver mutation stayed green: report-digest-lie"
  fi
  if [[ "$driver_rc" -eq 0 ]]; then
    grep -F "$expected" "$case_root/checker.out" >/dev/null \
      || fail "report-digest-lie missed expected checker guard: $expected"
  fi
}


expect_inventory_executable_flag_removed() {
  local case_root="$scratch/inventory-executable-flag-removed"
  local expected="harness executable surface drifted"
  mkdir -p "$case_root"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$case_root/manifest.json" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
manifest = json.loads(path.read_text(encoding="utf-8"))
helper = "scripts/ci/release_archive_inventory.sh"
found = False
for row in manifest.get("files") or []:
    if row.get("path") == helper:
        row["executable"] = False
        found = True
if not found:
    raise SystemExit("inventory helper row missing from manifest")
path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  if python3 "$CHECKER" \
      --workflow "$WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$case_root/manifest.json" \
      --source-root "$ROOT" >"$case_root/output" 2>&1; then
    fail "source mutation stayed green: inventory-executable-flag-removed"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "inventory-executable-flag-removed missed expected guard: $expected"
}

expect_coordinated_executable_escape() {
  local case_root="$scratch/coordinated-executable-escape"
  local expected="harness executable surface drifted"
  mkdir -p "$case_root"
  cp "$WORKFLOW" "$case_root/workflow.yml"
  cp "$DRIVER" "$case_root/driver.sh"
  cp "$CHECKER" "$case_root/checker.py"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$case_root/manifest.json" "$case_root/checker.py" <<'PY'
import json, pathlib, sys
manifest_path, checker_path = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
helper = "scripts/ci/release_archive_inventory.sh"
found = False
for row in manifest.get("files") or []:
    if row.get("path") == helper:
        row["executable"] = False
        found = True
if not found:
    raise SystemExit("inventory helper row missing from manifest")
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
text = checker_path.read_text(encoding="utf-8")
old = 'V1_EXECUTABLE_PATHS = ["scripts/ci/release_archive_inventory.sh"]'
new = "V1_EXECUTABLE_PATHS = []"
if old in text:
    if text.count(old) != 1:
        raise SystemExit(f"executable_paths pin count: {text.count(old)}")
    text = text.replace(old, new, 1)
checker_path.write_text(text, encoding="utf-8")
PY
  if python3 "$CHECKER" \
      --workflow "$case_root/workflow.yml" \
      --driver "$case_root/driver.sh" \
      --manifest "$case_root/manifest.json" \
      --source-root "$ROOT" >"$case_root/live-checker.out" 2>&1; then
    fail "source mutation stayed green: coordinated-executable-escape live-checker"
  fi
  grep -F "$expected" "$case_root/live-checker.out" >/dev/null \
    || fail "coordinated-executable-escape live-checker missed expected guard: $expected"
  if python3 "$case_root/checker.py" \
      --workflow "$WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$MANIFEST" \
      --source-root "$ROOT" \
      --pre-commit "$ROOT/.pre-commit-config.yaml" >"$case_root/mutated-checker.out" 2>&1; then
    fail "source mutation stayed green: coordinated-executable-escape mutated-checker"
  fi
  grep -F "$expected" "$case_root/mutated-checker.out" >/dev/null \
    || fail "coordinated-executable-escape mutated-checker missed expected guard: $expected"
}

expect_omitted_reviewed_inventory_helper() {
  local case_root="$scratch/omitted-release-archive-inventory"
  local expected="harness manifest must list exactly the reviewed harness inputs"
  mkdir -p "$case_root"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$case_root/manifest.json" <<'PY'
import json, pathlib, sys
path = pathlib.Path(sys.argv[1])
manifest = json.loads(path.read_text(encoding="utf-8"))
helper = "scripts/ci/release_archive_inventory.sh"
manifest["files"] = [row for row in manifest.get("files") or [] if row.get("path") != helper]
path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  if python3 "$CHECKER" \
      --workflow "$WORKFLOW" \
      --driver "$DRIVER" \
      --manifest "$case_root/manifest.json" \
      --source-root "$ROOT" >"$case_root/output" 2>&1; then
    fail "source mutation stayed green: omitted-release-archive-inventory"
  fi
  grep -F "$expected" "$case_root/output" >/dev/null \
    || fail "omitted-release-archive-inventory missed expected guard: $expected"
}

expect_coordinated_assay_version_v54_removal() {
  local case_root="$scratch/coordinated-drop-assay-version-v54"
  mkdir -p "$case_root"
  cp "$WORKFLOW" "$case_root/workflow.yml"
  cp "$DRIVER" "$case_root/driver.sh"
  cp "$MANIFEST" "$case_root/manifest.json"
  python3 - "$case_root/driver.sh" "$case_root/manifest.json" <<'PY'
import hashlib, json, pathlib, sys
driver, manifest_path = map(pathlib.Path, sys.argv[1:])
text = driver.read_text(encoding="utf-8")
old = (
    'run_capture "assay-version-v5.4" "observe" 0 "$results/assay-version-v54.txt" '
    '"$results/assay-version-v54.stderr" \\\n'
    '  "$active_link/bin/assay" version\n'
    '[[ "$(tr -d \'\\r\\n\' <"$results/assay-version-v54.txt")" == "$to_version" ]] '
    '|| fail "active version after v5.4 activation must be $to_version"\n'
)
if text.count(old) != 1:
    raise SystemExit(f"assay-version-v5.4 driver anchor count: {text.count(old)}")
driver.write_text(text.replace(old, "", 1), encoding="utf-8")
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
manifest["command_classes"]["observe"].remove("assay-version-v5.4")
for row in manifest["files"]:
    if row["path"] == "scripts/ci/published-release-historical-retention.sh":
        row["sha256"] = hashlib.sha256(driver.read_bytes()).hexdigest()
manifest_path.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
  if python3 "$CHECKER" \
      --workflow "$case_root/workflow.yml" \
      --driver "$case_root/driver.sh" \
      --manifest "$case_root/manifest.json" \
      --source-root "$ROOT" >"$case_root/source.out" 2>&1; then
    fail "source mutation stayed green: coordinated-drop-assay-version-v54"
  fi
  grep -F "harness manifest command_classes drifted from the v1 denominator" \
    "$case_root/source.out" >/dev/null \
    || fail "coordinated-drop-assay-version-v54 missed v1 denominator guard"
  python3 - "$MANIFEST" "$good_root/results" "$case_root" <<'PY'
import json, pathlib, shutil, sys
manifest_src, results_src, dest = map(pathlib.Path, sys.argv[1:])
results = dest / "results"
shutil.copytree(results_src, results)
manifest = json.loads(manifest_src.read_text(encoding="utf-8"))
manifest["command_classes"]["observe"].remove("assay-version-v5.4")
(dest / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
rows = [json.loads(line) for line in (results / "commands.ndjson").read_text(encoding="utf-8").splitlines() if line]
kept = [row for row in rows if row.get("name") != "assay-version-v5.4"]
if len(kept) != len(rows) - 1:
    raise SystemExit("coordinated-drop-assay-version-v54 could not drop the observe row")
(results / "commands.ndjson").write_text(
    "".join(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n" for row in kept),
    encoding="utf-8",
)
PY
  if python3 "$CHECKER" \
      --results "$case_root/results" \
      --manifest "$case_root/manifest.json" \
      --expected-head-sha "$EXPECTED_HEAD_SHA" >"$case_root/results.out" 2>&1; then
    fail "results mutation stayed green: coordinated-drop-assay-version-v54"
  fi
  grep -F "exact-once class assay-version-v5.4 occurred 0 times" "$case_root/results.out" >/dev/null \
    || fail "coordinated-drop-assay-version-v54 missed exact-once reject"
}

fixture="$scratch/fixture"
build_fixture_root "$fixture"
good_root="$scratch/good"
run_driver "$good_root" "$fixture"
check_results "$good_root"
expect_materialized_helper_modes
if python3 "$CHECKER" --results "$good_root/results" --manifest "$MANIFEST" \
    >"$scratch/missing-expected-head.out" 2>&1; then
  fail "results mutation stayed green: missing-expected-head-sha"
fi
grep -F -- "--expected-head-sha is required for --results" "$scratch/missing-expected-head.out" >/dev/null \
  || fail "missing-expected-head-sha missed required flag"

# no-op GREEN
cp -R "$good_root/results" "$scratch/noop-results"
check_results "$good_root"
python3 "$CHECKER" --results "$scratch/noop-results" --manifest "$MANIFEST" \
  --expected-head-sha "$EXPECTED_HEAD_SHA"

# byte-identical copy-aside/restore GREEN (explicit control, outside the claim)
copy_root="$scratch/copy-restore"
mkdir -p "$copy_root"
cp -R "$good_root/results" "$copy_root/results"
cp -R "$good_root/journey" "$copy_root/aside-journey"
rm -rf "$copy_root/restored-journey"
cp -R "$copy_root/aside-journey" "$copy_root/restored-journey"
python3 "$CHECKER" --results "$copy_root/results" --manifest "$MANIFEST" \
  --expected-head-sha "$EXPECTED_HEAD_SHA"

python3 - "$MANIFEST" "$good_root/results" "$scratch" <<'PY'
import json, pathlib, shutil, sys
manifest_path, src, scratch = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
required = manifest.get("required_retained_artifacts")
if not isinstance(required, list) or not required:
    raise SystemExit("manifest must declare required_retained_artifacts")

def load(path):
    return [json.loads(line) for line in path.read_text(encoding="utf-8").splitlines() if line]

def dump(path, rows):
    path.write_text("".join(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n" for row in rows), encoding="utf-8")

commands = load(src / "commands.ndjson")
ledger = load(src / "journey-ledger.ndjson")

dup_root = scratch / "dup-class"
shutil.copytree(src, dup_root)
dup = dict(commands[next(i for i, row in enumerate(commands) if row["name"] == "init")])
dup["argv"] = list(dup["argv"]) + ["--format", "json"]
dump(dup_root / "commands.ndjson", commands + [dup])

canary_root = scratch / "canary-change"
shutil.copytree(src, canary_root)
changed = list(ledger)
changed[-1] = dict(changed[-1])
changed[-1]["canary_sha256"] = "ab" * 32
dump(canary_root / "journey-ledger.ndjson", changed)

digest_root = scratch / "digest-change"
shutil.copytree(src, digest_root)
digest_rows = list(ledger)
last = dict(digest_rows[-1])
files = [dict(row) for row in last["files"]]
files[0]["sha256"] = "cd" * 32
last["files"] = files
digest_rows[-1] = last
dump(digest_root / "journey-ledger.ndjson", digest_rows)

missing_root = scratch / "missing-boundary"
shutil.copytree(src, missing_root)
dump(missing_root / "journey-ledger.ndjson", ledger[:-1])

failed_target = scratch / "failed-points-v54"
shutil.copytree(src, failed_target)
retargeted = []
for row in ledger:
    item = dict(row)
    if item.get("boundary") == "failed-v5.4-activation":
        item["activation_target"] = "v5.4.0"
    retargeted.append(item)
dump(failed_target / "journey-ledger.ndjson", retargeted)

self_attested = scratch / "self-attested"
shutil.copytree(src, self_attested)
pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
pin["continuity_matched"] = True
(self_attested / "run-pin.json").write_text(json.dumps(pin, indent=2) + "\n", encoding="utf-8")

deleted = scratch / "later-boundary-deletion"
shutil.copytree(src, deleted)
deleted_rows = list(ledger)
last = dict(deleted_rows[-1])
kept = [dict(row) for row in last["files"] if row.get("path") != "session/eval.yaml"]
if len(kept) == len(last["files"]):
    raise SystemExit("later-boundary-deletion could not omit a v5.3-created file")
last["files"] = kept
deleted_rows[-1] = last
dump(deleted / "journey-ledger.ndjson", deleted_rows)

deleted_v1 = scratch / "later-v1-bundle-deletion"
shutil.copytree(src, deleted_v1)
v1_rows = list(ledger)
v1_last = dict(v1_rows[-1])
v1_kept = [dict(row) for row in v1_last["files"] if row.get("path") != "results/v1.bundle"]
if len(v1_kept) == len(v1_last["files"]):
    raise SystemExit("later-v1-bundle-deletion could not omit results/v1.bundle")
v1_last["files"] = v1_kept
v1_rows[-1] = v1_last
dump(deleted_v1 / "journey-ledger.ndjson", v1_rows)

false_stage = scratch / "false-stage-executed"
shutil.copytree(src, false_stage)
false_stage_rows = []
for row in commands:
    item = dict(row)
    if item.get("name") == "stage-prefix-v5.3.0":
        item["executed_binary_sha256"] = item.get("subject_binary_sha256") or ("ee" * 32)
    false_stage_rows.append(item)
dump(false_stage / "commands.ndjson", false_stage_rows)

false_activate = scratch / "false-activate-executed"
shutil.copytree(src, false_activate)
false_activate_rows = []
for row in commands:
    item = dict(row)
    if item.get("name") == "failed-activate-v5.4":
        item["executed_binary_sha256"] = item.get("subject_binary_sha256") or ("ff" * 32)
    false_activate_rows.append(item)
dump(false_activate / "commands.ndjson", false_activate_rows)

dropped_activate = scratch / "dropped-activate"
shutil.copytree(src, dropped_activate)
dump(
    dropped_activate / "commands.ndjson",
    [row for row in commands if row.get("name") != "failed-activate-v5.4"],
)

dropped_verify_v1 = scratch / "dropped-verify-v1"
shutil.copytree(src, dropped_verify_v1)
dump(
    dropped_verify_v1 / "commands.ndjson",
    [row for row in commands if row.get("name") != "verify-v1-under-v5.4"],
)

dropped_migrate_v54 = scratch / "dropped-migrate-v54"
shutil.copytree(src, dropped_migrate_v54)
dump(
    dropped_migrate_v54 / "commands.ndjson",
    [row for row in commands if row.get("name") != "migrate-check-v5.4"],
)

empty_prov = scratch / "empty-provenance"
shutil.copytree(src, empty_prov)
empty_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
empty_pin["image_os"] = ""
(empty_prov / "run-pin.json").write_text(json.dumps(empty_pin, indent=2) + "\n", encoding="utf-8")

missing_exit = scratch / "missing-recorded-exit"
shutil.copytree(src, missing_exit)
exit_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
exit_pin["v0_cross_version_verify"] = dict(exit_pin.get("v0_cross_version_verify") or {})
exit_pin["v0_cross_version_verify"].pop("recorded_exit", None)
(missing_exit / "run-pin.json").write_text(json.dumps(exit_pin, indent=2) + "\n", encoding="utf-8")

missing_harness = scratch / "missing-harness"
shutil.copytree(src, missing_harness)
missing_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
missing_pin.pop("harness", None)
(missing_harness / "run-pin.json").write_text(json.dumps(missing_pin, indent=2) + "\n", encoding="utf-8")

forged_driver = scratch / "forged-driver-sha256"
shutil.copytree(src, forged_driver)
forged_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
forged_pin["harness"] = dict(forged_pin.get("harness") or {})
forged_pin["harness"]["driver_sha256"] = "ab" * 32
(forged_driver / "run-pin.json").write_text(json.dumps(forged_pin, indent=2) + "\n", encoding="utf-8")

wrong_head = scratch / "wrong-head-sha"
shutil.copytree(src, wrong_head)
wrong_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
wrong_pin["harness"] = dict(wrong_pin.get("harness") or {})
wrong_pin["harness"]["head_sha"] = "aa" * 20
(wrong_head / "run-pin.json").write_text(json.dumps(wrong_pin, indent=2) + "\n", encoding="utf-8")

empty_run_id = scratch / "empty-workflow-run-id"
shutil.copytree(src, empty_run_id)
empty_id_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
empty_id_pin["harness"] = dict(empty_id_pin.get("harness") or {})
empty_id_pin["harness"]["workflow_run_id"] = ""
(empty_run_id / "run-pin.json").write_text(json.dumps(empty_id_pin, indent=2) + "\n", encoding="utf-8")

zero_attempt = scratch / "zero-workflow-run-attempt"
shutil.copytree(src, zero_attempt)
zero_pin = json.loads((src / "run-pin.json").read_text(encoding="utf-8"))
zero_pin["harness"] = dict(zero_pin.get("harness") or {})
zero_pin["harness"]["workflow_run_attempt"] = 0
(zero_attempt / "run-pin.json").write_text(json.dumps(zero_pin, indent=2) + "\n", encoding="utf-8")

paraphrased = scratch / "paraphrased-canary"
shutil.copytree(src, paraphrased)
paraphrased_rows = []
for row in commands:
    item = dict(row)
    if item.get("name") == "create-journey-canary":
        item["argv"] = [item.get("argv", ["python3"])[0], "-c", "os.urandom", "journey/.journey-canary"]
    paraphrased_rows.append(item)
dump(paraphrased / "commands.ndjson", paraphrased_rows)

undeclared = scratch / "undeclared-command"
shutil.copytree(src, undeclared)
dump(
    undeclared / "commands.ndjson",
    commands
    + [
        {
            "name": "arbitrary-undeclared",
            "class": "observe",
            "exit_code": 0,
            "argv": ["true"],
            "stdout_sha256": "aa" * 32,
            "stderr_sha256": "bb" * 32,
            "executed_binary_sha256": "cc" * 32,
            "selected_profile": "v0",
        }
    ],
)

class_mismatch = scratch / "class-mismatch"
shutil.copytree(src, class_mismatch)
mismatched = []
for row in commands:
    item = dict(row)
    if item.get("name") == "init":
        item["class"] = "observe"
    mismatched.append(item)
dump(class_mismatch / "commands.ndjson", mismatched)

for path in required:
    dest = scratch / ("deleted-required-" + path.replace("/", "_"))
    shutil.copytree(src, dest)
    stripped = []
    removed = False
    for row in ledger:
        item = dict(row)
        kept = [dict(file_row) for file_row in item.get("files", []) if file_row.get("path") != path]
        if len(kept) != len(item.get("files", [])):
            removed = True
        item["files"] = kept
        stripped.append(item)
    if not removed:
        raise SystemExit(f"deleted-required could not omit {path}")
    dump(dest / "journey-ledger.ndjson", stripped)
PY

expect_results_failure "same-class-different-argv" "$scratch/dup-class" "exact-once class init occurred 2 times"
expect_results_failure "recreated-canary" "$scratch/canary-change" "canary continuity failed"
expect_results_failure "one-changed-digest" "$scratch/digest-change" "pairwise byte continuity failed"
expect_results_failure "missing-boundary" "$scratch/missing-boundary" "required boundaries drifted"
expect_results_failure "failed-v54-points-at-v54" "$scratch/failed-points-v54" "failed-v5.4-activation must keep active on v5.3.0"
expect_results_failure "self-attested-verdict" "$scratch/self-attested" "run-pin contains self-attested verdict continuity_matched"
expect_results_failure "later-boundary-deletion" "$scratch/later-boundary-deletion" "later boundary omitted retained file session/eval.yaml at v5.3-reactivated"
expect_results_failure "later-v1-bundle-deletion" "$scratch/later-v1-bundle-deletion" "later boundary omitted retained file results/v1.bundle at v5.3-reactivated"
expect_results_failure "false-stage-executed" "$scratch/false-stage-executed" "staging row must not claim executed_binary_sha256"
expect_results_failure "false-activate-executed" "$scratch/false-activate-executed" "failed-activate-v5.4 must not claim executed_binary_sha256"
expect_results_failure "dropped-activate" "$scratch/dropped-activate" "exact-once class failed-activate-v5.4 occurred 0 times"
expect_results_failure "dropped-verify-v1" "$scratch/dropped-verify-v1" "exact-once class verify-v1-under-v5.4 occurred 0 times"
expect_results_failure "dropped-migrate-v54" "$scratch/dropped-migrate-v54" "exact-once class migrate-check-v5.4 occurred 0 times"
expect_results_failure "empty-provenance" "$scratch/empty-provenance" "run-pin provenance must be non-empty: image_os"
expect_results_failure "missing-recorded-exit" "$scratch/missing-recorded-exit" "v0 cross-version verify recorded_exit must be an integer"
expect_results_failure "missing-harness" "$scratch/missing-harness" "run-pin harness is missing"
expect_results_failure "forged-driver-sha256" "$scratch/forged-driver-sha256" \
  "run-pin harness.driver_sha256 must match the harness manifest driver digest"
expect_results_failure "wrong-head-sha" "$scratch/wrong-head-sha" \
  "run-pin harness.head_sha must match --expected-head-sha"
expect_results_failure "empty-workflow-run-id" "$scratch/empty-workflow-run-id" \
  "run-pin harness.workflow_run_id must be a nonempty string"
expect_results_failure "zero-workflow-run-attempt" "$scratch/zero-workflow-run-attempt" \
  "run-pin harness.workflow_run_attempt must be a positive integer"
expect_results_failure "paraphrased-canary" "$scratch/paraphrased-canary" "canary row must record the argv that actually ran"
expect_results_failure "undeclared-command" "$scratch/undeclared-command" "undeclared command: arbitrary-undeclared"
expect_results_failure "class-mismatch" "$scratch/class-mismatch" "command init class observe does not match manifest class state_producing"
expect_results_failure "deleted-required-v0-bundle" "$scratch/deleted-required-results_v0.bundle" "required retained artifact missing: results/v0.bundle"
expect_results_failure "deleted-required-v1-bundle" "$scratch/deleted-required-results_v1.bundle" "required retained artifact missing: results/v1.bundle"
expect_results_failure "deleted-required-canary" "$scratch/deleted-required-journey_.journey-canary" "required retained artifact missing: journey/.journey-canary"
expect_results_failure "deleted-required-config" "$scratch/deleted-required-home_.config_assay_config.toml" "required retained artifact missing: home/.config/assay/config.toml"
expect_results_failure "deleted-required-policy" "$scratch/deleted-required-session_policy.yaml" "required retained artifact missing: session/policy.yaml"
expect_results_failure "deleted-required-eval" "$scratch/deleted-required-session_eval.yaml" "required retained artifact missing: session/eval.yaml"
expect_results_failure "deleted-required-trace" "$scratch/deleted-required-session_traces_hello.jsonl" "required retained artifact missing: session/traces/hello.jsonl"

python3 - "$good_root/results" "$scratch" <<'PY'
import json, pathlib, shutil, sys
src, scratch = map(pathlib.Path, sys.argv[1:])
HELPER = "scripts/ci/release_archive_inventory.sh"
CONTROL = "scripts/ci/bounded_download.py"

def copy_results(name):
    dest = scratch / name
    if dest.exists():
        shutil.rmtree(dest)
    shutil.copytree(src, dest)
    return dest

def load_report(dest):
    path = dest / "harness-files.json"
    return path, json.loads(path.read_text(encoding="utf-8"))

def write_report(path, report):
    path.write_text(json.dumps(report, indent=2, sort_keys=True) + "\n", encoding="utf-8")

def set_flag(report, path, value):
    found = False
    for row in report.get("files") or []:
        if row.get("path") == path:
            row["executable"] = value
            found = True
    if not found:
        raise SystemExit(f"missing harness-files row: {path}")

dest = copy_results("harness-files-true-to-false")
path, report = load_report(dest)
set_flag(report, HELPER, False)
write_report(path, report)

dest = copy_results("harness-files-false-to-true")
path, report = load_report(dest)
set_flag(report, CONTROL, True)
write_report(path, report)

dest = copy_results("harness-files-missing")
(dest / "harness-files.json").unlink()

dest = copy_results("harness-files-malformed-flag")
path, report = load_report(dest)
set_flag(report, HELPER, "true")
write_report(path, report)

dest = copy_results("harness-files-duplicate-path")
path, report = load_report(dest)
files = list(report.get("files") or [])
helper = next(row for row in files if row.get("path") == HELPER)
files.append(dict(helper))
report["files"] = files
write_report(path, report)

dest = copy_results("harness-files-unknown-path")
path, report = load_report(dest)
files = list(report.get("files") or [])
files.append({"path": "scripts/ci/not-in-manifest.sh", "sha256": "0" * 64, "executable": False})
report["files"] = files
write_report(path, report)

dest = copy_results("harness-files-missing-path")
path, report = load_report(dest)
report["files"] = [row for row in report.get("files") or [] if row.get("path") != HELPER]
write_report(path, report)

dest = copy_results("harness-files-helper-digest-zeroed")
path, report = load_report(dest)
found = False
for row in report.get("files") or []:
    if row.get("path") == HELPER:
        row["sha256"] = "0" * 64
        found = True
if not found:
    raise SystemExit("missing helper row for digest tamper")
write_report(path, report)
PY

expect_results_failure "harness-files-true-to-false" "$scratch/harness-files-true-to-false" \
  "harness files observation drifted"
expect_results_failure "harness-files-false-to-true" "$scratch/harness-files-false-to-true" \
  "harness files observation drifted"
expect_results_failure "harness-files-missing" "$scratch/harness-files-missing" \
  "harness-files.json is unreadable"
expect_results_failure "harness-files-malformed-flag" "$scratch/harness-files-malformed-flag" \
  "invalid executable flag"
expect_results_failure "harness-files-duplicate-path" "$scratch/harness-files-duplicate-path" \
  "harness-files.json path is duplicated"
expect_results_failure "harness-files-unknown-path" "$scratch/harness-files-unknown-path" \
  "harness-files.json path is unknown"
expect_results_failure "harness-files-missing-path" "$scratch/harness-files-missing-path" \
  "harness-files.json missing path"
expect_results_failure "harness-files-helper-digest-zeroed" "$scratch/harness-files-helper-digest-zeroed" \
  "harness files observation drifted"


python3 - "$MANIFEST" "$good_root/results" "$scratch" <<'PY'
import json, pathlib, shutil, sys
manifest_src, results_src, scratch = map(pathlib.Path, sys.argv[1:])

def write_manifest(dest, document):
    dest.write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")

src_manifest = json.loads(manifest_src.read_text(encoding="utf-8"))

from_tag = scratch / "from-tag-drift.json"
from_doc = dict(src_manifest)
from_doc["from_tag"] = "v9.9.9"
write_manifest(from_tag, from_doc)

to_tag = scratch / "to-tag-drift.json"
to_doc = dict(src_manifest)
to_doc["to_tag"] = "v9.9.8"
write_manifest(to_tag, to_doc)

omitted = scratch / "omitted-boundary-mapping.json"
omitted_doc = dict(src_manifest)
refs = dict(omitted_doc.get("boundary_activation_refs") or {})
names = list(omitted_doc.get("boundaries") or [])
if not refs:
    refs = {name: "activate-v5.4" for name in names}
if names:
    refs.pop(names[0], None)
omitted_doc["boundary_activation_refs"] = refs
write_manifest(omitted, omitted_doc)
PY

expect_results_failure_with_manifest \
  "from-tag-drift" "$good_root/results" "$scratch/from-tag-drift.json" \
  "run-pin from_tag must match the harness manifest"
expect_results_failure_with_manifest \
  "to-tag-drift" "$good_root/results" "$scratch/to-tag-drift.json" \
  "run-pin to_tag must match the harness manifest"
expect_results_failure_with_manifest \
  "omitted-boundary-mapping" "$good_root/results" "$scratch/omitted-boundary-mapping.json" \
  "boundary activation refs must list every boundary exactly once"

python3 - "$MANIFEST" "$good_root/results" "$scratch" <<'PY'
import json, pathlib, shutil, sys
manifest_src, results_src, scratch = map(pathlib.Path, sys.argv[1:])
exempt = scratch / "optional-observe-escape"
shutil.copytree(results_src, exempt)
manifest = json.loads(manifest_src.read_text(encoding="utf-8"))
manifest["optional_command_classes"] = ["observe"]
(exempt / "manifest.json").write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
dropped = {
    "migrate-check-v5.3",
    "migrate-check-v5.4",
    "verify-v1-under-v5.4",
    "post-reactivation-active",
}
rows = [json.loads(line) for line in (exempt / "commands.ndjson").read_text(encoding="utf-8").splitlines() if line]
kept = [row for row in rows if row.get("name") not in dropped]
if len(kept) != len(rows) - 4:
    raise SystemExit("optional-observe-escape could not drop the four observe rows")
(exempt / "commands.ndjson").write_text(
    "".join(json.dumps(row, sort_keys=True, separators=(",", ":")) + "\n" for row in kept),
    encoding="utf-8",
)
PY
if python3 "$CHECKER" --results "$scratch/optional-observe-escape" \
    --manifest "$scratch/optional-observe-escape/manifest.json" \
    --expected-head-sha "$EXPECTED_HEAD_SHA" \
    >"$scratch/optional-observe-escape.out" 2>&1; then
  fail "results mutation stayed green: optional-observe-escape"
fi
grep -F "exact-once class migrate-check-v5.4 occurred 0 times" "$scratch/optional-observe-escape.out" >/dev/null \
  || fail "optional-observe-escape missed exact-once reject for observe"

python3 - "$MANIFEST" "$scratch/blind-manifest.json" <<'PY'
import json, pathlib, sys
src, dest = map(pathlib.Path, sys.argv[1:])
manifest = json.loads(src.read_text(encoding="utf-8"))
manifest["command_classes"] = {"observe": ["init"]}
dest.write_text(json.dumps(manifest, indent=2) + "\n", encoding="utf-8")
PY
if python3 "$CHECKER" --results "$good_root/results" --manifest "$scratch/blind-manifest.json" \
    --expected-head-sha "$EXPECTED_HEAD_SHA" \
    >"$scratch/blind-class.out" 2>&1; then
  fail "checker-blindness mutation stayed green: removed state_producing"
fi
grep -F "harness manifest command_classes drifted from the v1 denominator" "$scratch/blind-class.out" >/dev/null \
  || fail "checker-blindness missed v1 denominator guard"

expect_source_failure \
  "canary-write-removed" "driver.sh" \
  "journey/.journey-canary" "journey/.not-a-canary" \
  "driver must create a once-only journey canary"
expect_source_failure \
  "ledger-write-removed" "driver.sh" \
  "journey-ledger.ndjson" "journey-notes.ndjson" \
  "driver must retain the journey ledger"
expect_source_failure \
  "activation-before-digest" "driver.sh" \
  "prefix digest mismatch before activation" "prefix later digest mismatch" \
  "activation must check digest and version before switching the symlink"
expect_source_failure \
  "value-rejected" "driver.sh" \
  "flag_unavailable" "value_rejected" \
  "v5.3 explicit v1 must not be classified as value_rejected"
expect_source_failure \
  "migration-omitted" "driver.sh" \
  '"migration_required": False' '"migration_omitted": False' \
  "driver must record migration_required false"
expect_source_failure \
  "measured-v0-verify" "driver.sh" \
  '"disposition": "unmeasured"' '"disposition": "asserted"' \
  "driver must not decide the unmeasured v0 cross-version verify"
expect_source_failure \
  "fixture-in-workflow" "workflow.yml" \
  '--run-root "$RUN_ROOT"' \
  $'--run-root "$RUN_ROOT" \\\n            --fixture-root /tmp/fixture' \
  "hosted workflow must not pass fixture-root"
expect_source_failure \
  "retention-30" "workflow.yml" \
  "retention-days: 90" "retention-days: 30" \
  "workflow must retain hosted artifacts for 90 days"
expect_source_failure \
  "claim-ceiling-overstated" "driver.sh" \
  "the harness observed single-creation byte continuity" \
  "the harness is a shipped updater with automatic rollback" \
  "driver lost claim ceiling"

workflow_checker_call='          python3 scripts/ci/check-published-release-historical-retention-contract.py '"\\"
workflow_checker_decoy=$'          # python3 scripts/ci/check-published-release-historical-retention-contract.py\n          echo skipped-consumer-checker '"\\"
expect_source_failure \
  "missing-activate-records" "driver.sh" \
  'record_activate "failed-activate-v5.4"' \
  'record_command "failed-activate-v5.4"' \
  "driver must record semantic activation rows"
expect_source_failure \
  "missing-runner-os" "workflow.yml" \
  "RUNNER_OS: \${{ runner.os }}" "IMAGE_OS: \${{ env.ImageOS }}" \
  "workflow must bind runner OS provenance"
expect_source_failure \
  "empty-image-os-runtime" "workflow.yml" \
  'IMAGE_OS="${ImageOS:?image os provenance is empty}"' \
  'IMAGE_OS=""' \
  "workflow must bind non-empty ImageOS at runtime"
expect_source_failure \
  "empty-image-version-runtime" "workflow.yml" \
  'IMAGE_VERSION="${ImageVersion:?image version provenance is empty}"' \
  'IMAGE_VERSION=""' \
  "workflow must bind non-empty ImageVersion at runtime"
expect_source_failure \
  "hosted-consumer-checker-commented" "workflow.yml" \
  "$workflow_checker_call" \
  "$workflow_checker_decoy" \
  "workflow must execute only the exact reviewed consumer-checker invocation"
expect_source_failure \
  "missing-expected-head-sha" "workflow.yml" \
  $'            --manifest scripts/ci/fixtures/published-release-historical-retention/v1/harness-manifest.json \\\n            --expected-head-sha "$GITHUB_SHA"' \
  "            --manifest scripts/ci/fixtures/published-release-historical-retention/v1/harness-manifest.json" \
  "workflow must execute only the exact reviewed consumer-checker invocation"
expect_source_failure \
  "trigger-schedule" "workflow.yml" \
  "  workflow_dispatch:" \
  $'  workflow_dispatch:\n  schedule:\n    - cron: "0 0 * * *"' \
  "historical workflow must be workflow_dispatch only"
expect_source_failure \
  "trigger-repository-dispatch" "workflow.yml" \
  "  workflow_dispatch:" \
  $'  workflow_dispatch:\n  repository_dispatch:' \
  "historical workflow must be workflow_dispatch only"
expect_source_failure \
  "trigger-workflow-run" "workflow.yml" \
  "  workflow_dispatch:" \
  $'  workflow_dispatch:\n  workflow_run:\n    types: [completed]' \
  "historical workflow must be workflow_dispatch only"
expect_source_failure \
  "harness-digest-drift" "manifest.json" \
  "08eff32101003614a9d5de93507c2d26ec087d1417179d34bea41a70ee4bafaa" \
  "18eff32101003614a9d5de93507c2d26ec087d1417179d34bea41a70ee4bafaa" \
  "harness digest drifted"

if hosted_consumer_check "$scratch/self-attested"; then
  fail "forged retained record kept hosted consumer-checker green"
fi

outside_readable="$scratch/outside-readable-sentinel"
printf 'SHOULD-NOT-READ\n' >"$outside_readable"
outside_unreadable="$scratch/outside-unreadable-sentinel"
printf 'SHOULD-NOT-READ\n' >"$outside_unreadable"
chmod 000 "$outside_unreadable"
escaped_tag="$scratch/escaped-tag-prefix"

expect_driver_unsafe_manifest \
  "unsafe-artifact-absolute" "artifact" "$outside_readable" \
  "unsafe required retained artifact path: $outside_readable"
expect_driver_unsafe_manifest \
  "unsafe-artifact-unreadable" "artifact" "$outside_unreadable" \
  "unsafe required retained artifact path: $outside_unreadable"
expect_driver_unsafe_manifest \
  "unsafe-artifact-dotdot" "artifact" "../../outside" \
  "unsafe required retained artifact path: ../../outside"
expect_driver_unsafe_manifest \
  "unsafe-from-tag-dotdot" "from_tag" "../escape" \
  "unsafe from_tag path component"
expect_driver_unsafe_manifest \
  "unsafe-from-tag-absolute" "from_tag" "$escaped_tag" \
  "unsafe from_tag path component"
[[ ! -e "$escaped_tag" ]] || fail "unsafe from_tag materialized an escaped prefix"
expect_driver_unsafe_manifest \
  "unsafe-to-tag-absolute" "to_tag" "$escaped_tag" \
  "unsafe to_tag path component"
[[ ! -e "$escaped_tag" ]] || fail "unsafe to_tag materialized an escaped prefix"
expect_driver_unsafe_manifest \
  "duplicate-required-artifact" "duplicate_artifact" unused \
  "required retained artifact path is duplicated"

expect_path_safety_parity \
  "from-tag-dotdot" "from_tag" "../escape" "unsafe from_tag path component"
expect_path_safety_parity \
  "to-tag-absolute" "to_tag" "/tmp/historical-retention-escape" "unsafe to_tag path component"
expect_path_safety_parity \
  "artifact-absolute" "artifact" "/etc/passwd" "unsafe required retained artifact path: /etc/passwd"
expect_path_safety_parity \
  "artifact-dotdot" "artifact" "../../outside" "unsafe required retained artifact path: ../../outside"
expect_path_safety_parity \
  "artifact-duplicate" "duplicate_artifact" unused "required retained artifact path is duplicated"
expect_driver_unsafe_manifest \
  "unsafe-artifact-dot" "artifact" "." \
  "unsafe required retained artifact path: ."
expect_path_safety_parity \
  "artifact-dot" "artifact" "." "unsafe required retained artifact path: ."
expect_path_safety_parity \
  "artifact-dot-slash" "artifact" "./x" "unsafe required retained artifact path: ./x"
expect_path_safety_parity \
  "artifact-double-separator" "artifact" "session//eval.yaml" \
  "unsafe required retained artifact path: session//eval.yaml"
expect_path_safety_parity \
  "artifact-backslash" "artifact" "session\\eval.yaml" \
  "unsafe required retained artifact path: session\\eval.yaml"
expect_path_safety_parity \
  "v1-release-pair" "release_pair" "v9.9.7,v9.9.8" \
  "harness manifest release pair drifted from the v1 denominator"

expect_omitted_reviewed_inventory_helper
expect_inventory_executable_flag_removed
expect_coordinated_executable_escape
expect_driver_neutralized_chmod
expect_driver_report_digest_lie
expect_source_failure \
  "inventory-executable-flag-string" "manifest.json" \
  '"executable": true' \
  '"executable": "true"' \
  "invalid executable flag"

while IFS= read -r helper; do
  expect_precommit_helper_decoy "$helper"
done < <(python3 - "$MANIFEST" <<'PY'
import json, pathlib, sys
manifest = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
skip = {
    ".github/workflows/published-release-historical-retention.yml",
    "scripts/ci/published-release-historical-retention.sh",
}
for row in manifest["files"]:
    path = row["path"]
    if path not in skip:
        print(path)
PY
)
expect_coordinated_assay_version_v54_removal

echo "ok: published-release historical-retention contract"
