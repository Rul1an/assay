#!/usr/bin/env bash
# Mutation anchors intentionally preserve literal shell and Actions expressions.
# shellcheck disable=SC2016
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
WORKFLOW="${WORKFLOW:-${ROOT}/.github/workflows/published-release-historical-retention.yml}"
DRIVER="${DRIVER:-${ROOT}/scripts/ci/published-release-historical-retention.sh}"
MANIFEST="${MANIFEST:-${ROOT}/scripts/ci/fixtures/published-release-historical-retention/v1/harness-manifest.json}"
CHECKER="${ROOT}/scripts/ci/check-published-release-historical-retention-contract.py"

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
    --harness-sha 6704d0bc4029f893f9558ab669ffc60918971943 \
    --workflow-run-id test-historical \
    --workflow-run-attempt 1 \
    --run-root "$run_root" \
    --fixture-root "$fixture"
}

hosted_consumer_check() {
  # Must stay identical to the hosted workflow consumer-checker argv.
  python3 "$CHECKER" \
    --results "$1" \
    --manifest "$MANIFEST"
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
  if python3 "$CHECKER" --results "$results" --manifest "$manifest" >"$scratch/$name.out" 2>&1; then
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
      --harness-sha 6704d0bc4029f893f9558ab669ffc60918971943 \
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

fixture="$scratch/fixture"
build_fixture_root "$fixture"
good_root="$scratch/good"
run_driver "$good_root" "$fixture"
check_results "$good_root"

# no-op GREEN
cp -R "$good_root/results" "$scratch/noop-results"
check_results "$good_root"
python3 "$CHECKER" --results "$scratch/noop-results" --manifest "$MANIFEST"

# byte-identical copy-aside/restore GREEN (explicit control, outside the claim)
copy_root="$scratch/copy-restore"
mkdir -p "$copy_root"
cp -R "$good_root/results" "$copy_root/results"
cp -R "$good_root/journey" "$copy_root/aside-journey"
rm -rf "$copy_root/restored-journey"
cp -R "$copy_root/aside-journey" "$copy_root/restored-journey"
python3 "$CHECKER" --results "$copy_root/results" --manifest "$MANIFEST"

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
if python3 "$CHECKER" --results "$scratch/optional-observe-escape" --manifest "$scratch/optional-observe-escape/manifest.json" \
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
    >"$scratch/blind-class.out" 2>&1; then
  fail "checker-blindness mutation stayed green: removed state_producing"
fi
grep -F "harness manifest has no state_producing command class" "$scratch/blind-class.out" >/dev/null \
  || fail "checker-blindness missed state_producing removal"

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

echo "ok: published-release historical-retention contract"
