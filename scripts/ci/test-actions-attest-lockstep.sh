#!/usr/bin/env bash
# Mutation battery for the closed two-workflow actions/attest pin set.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CHECKER="scripts/ci/check-actions-attest-lockstep.py"
PRECOMMIT=".pre-commit-config.yaml"
WORKFLOWS=(
  .github/workflows/runner-spike-delegated.yml
  .github/workflows/privileged-mcp-action-pack-release.yml
)
OWNER_SHA="1e69f48acb82d1966a394da916b4c1698aa569d6"
DRIFT_SHA="d1ba80a13dd99fba24a470575428917156a28b43"
HOOK_ENTRY="bash scripts/ci/test-actions-attest-lockstep.sh"
HOOK_FILES='^(\.github/workflows/.*\.ya?ml|scripts/ci/(assay_runner_delegated_proof_pack\.py|check-actions-attest-lockstep\.py|test-actions-attest-lockstep\.sh)|\.pre-commit-config\.yaml)$'

scratch="$(mktemp -d)"
trap 'rm -rf "$scratch"' EXIT

seed() {
  local dest="$1" path
  mkdir -p "$dest/scripts/ci" "$dest/.github/workflows"
  cp "$ROOT/$CHECKER" "$dest/$CHECKER"
  cp "$ROOT/$PRECOMMIT" "$dest/$PRECOMMIT"
  for path in "${WORKFLOWS[@]}"; do
    cp "$ROOT/$path" "$dest/$path"
  done
}

check_hook_scope() {
  local root="$1"
  python3 - "$root/$PRECOMMIT" "$HOOK_ENTRY" "$HOOK_FILES" <<'PY'
import re
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
expected_entry = sys.argv[2]
expected_files = sys.argv[3]
if "- id: actions-attest-lockstep" not in text:
    raise SystemExit("actions/attest lockstep hook is missing")
block = text.split("- id: actions-attest-lockstep", 1)[1].split("\n      - id:", 1)[0]
entry_match = re.search(r"^[ \t]*entry:[ \t]*(.+)$", block, re.MULTILINE)
if entry_match is None:
    raise SystemExit("actions/attest lockstep hook has no entry")
entry = entry_match.group(1).strip()
if entry != expected_entry:
    raise SystemExit(
        f"actions/attest lockstep hook entry {entry!r}, want {expected_entry!r}"
    )
match = re.search(r"^[ \t]*files:[ \t]*(.+)$", block, re.MULTILINE)
if match is None:
    raise SystemExit("actions/attest lockstep hook has no files selector")
pattern = match.group(1).strip()
if pattern != expected_files:
    raise SystemExit(
        f"actions/attest lockstep hook files {pattern!r}, want {expected_files!r}"
    )
required = (
    ".github/workflows/third-attest.yml",
    ".github/workflows/third-attest.yaml",
    "scripts/ci/assay_runner_delegated_proof_pack.py",
    "scripts/ci/check-actions-attest-lockstep.py",
    "scripts/ci/test-actions-attest-lockstep.sh",
    ".pre-commit-config.yaml",
)
missing = [path for path in required if re.search(pattern, path) is None]
if missing:
    raise SystemExit(f"actions/attest lockstep hook does not trigger for: {', '.join(missing)}")
PY
}

run_hook_scope_case() {
  local name="$1" root="$2" expected="$3" status=0
  check_hook_scope "$root" >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

run_case() {
  local name="$1" root="$2" expected="$3" status=0
  (cd "$root" && python3 "$CHECKER") >"$scratch/$name.log" 2>&1 || status=$?
  if [[ "$status" -ne "$expected" ]]; then
    cat "$scratch/$name.log" >&2
    echo "FAIL: $name exited $status, wanted $expected" >&2
    exit 1
  fi
  echo "ok    $name (exit $status)"
}

mutate_once() {
  local path="$1" old="$2" new="$3"
  python3 - "$path" "$old" "$new" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
old, new = sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation subject count is {text.count(old)}, want 1: {old}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

case_root="$scratch/control"
seed "$case_root"
run_case control-is-green "$case_root" 0
run_hook_scope_case hook-scope-covers-all-workflows "$case_root" 0

case_root="$scratch/hook-entry-true"
seed "$case_root"
mutate_once \
  "$case_root/$PRECOMMIT" \
  "        entry: ${HOOK_ENTRY}" \
  "        entry: true"
run_hook_scope_case hook-entry-true-is-refused "$case_root" 1

case_root="$scratch/narrow-hook-scope"
seed "$case_root"
mutate_once \
  "$case_root/$PRECOMMIT" \
  "files: ${HOOK_FILES}" \
  'files: ^(\.github/workflows/(runner-spike-delegated|privileged-mcp-action-pack-release)\.yml|scripts/ci/(assay_runner_delegated_proof_pack\.py|check-actions-attest-lockstep\.py|test-actions-attest-lockstep\.sh)|\.pre-commit-config\.yaml)$'
run_hook_scope_case narrow-hook-scope-is-refused "$case_root" 1

case_root="$scratch/missing-hook"
seed "$case_root"
python3 - "$case_root/$PRECOMMIT" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
start = text.find("      - id: actions-attest-lockstep")
if start < 0:
    raise SystemExit("hook block missing")
end = text.find("      - id:", start + 1)
if end < 0:
    raise SystemExit("next hook id missing")
path.write_text(text[:start] + text[end:], encoding="utf-8")
PY
run_hook_scope_case missing-hook-is-refused "$case_root" 1

case_root="$scratch/one-laggard"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "$OWNER_SHA" \
  "$DRIFT_SHA"
run_case one-laggard-is-refused "$case_root" 1

case_root="$scratch/tag-drift"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "# v4.2.2" \
  "# v4.2.1"
run_case tag-drift-is-refused "$case_root" 1

case_root="$scratch/retarget-delegated-subject"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "          subject-checksums: assay-runner-proof-upload/subject-checksums.txt" \
  "          subject-checksums: assay-runner-proof-upload/retargeted-checksums.txt"
run_case retarget-delegated-subject-is-refused "$case_root" 1

case_root="$scratch/retarget-pack-subject"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "subject-checksums: release/SHA256SUMS" \
  "subject-checksums: release/OTHERSUMS"
run_case retarget-pack-subject-is-refused "$case_root" 1

case_root="$scratch/delete-delegated-subject"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "          subject-checksums: assay-runner-proof-upload/subject-checksums.txt
" \
  ""
run_case delete-delegated-subject-is-refused "$case_root" 1

case_root="$scratch/delete-pack-subject"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "          subject-checksums: release/SHA256SUMS
" \
  ""
run_case delete-pack-subject-is-refused "$case_root" 1

case_root="$scratch/unwired-with"
seed "$case_root"
python3 - "$case_root/.github/workflows/runner-spike-delegated.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2
        with:
          subject-checksums: assay-runner-proof-upload/subject-checksums.txt
          show-summary: true
"""
new = """        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2
"""
if text.count(old) != 1:
    raise SystemExit("unwired-with subject missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
run_case sha-only-unwired-with-is-refused "$case_root" 1

case_root="$scratch/remove-delegated-uses"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        uses: actions/attest@${OWNER_SHA} # v4.2.2" \
  "        run: echo bypassed-attest"
run_case remove-delegated-uses-is-refused "$case_root" 1

case_root="$scratch/remove-pack-uses"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        uses: actions/attest@${OWNER_SHA} # v4.2.2" \
  "        run: echo bypassed-attest"
run_case remove-pack-uses-is-refused "$case_root" 1

case_root="$scratch/remove-delegated-workflow"
seed "$case_root"
rm "$case_root/.github/workflows/runner-spike-delegated.yml"
run_case remove-delegated-workflow-is-refused "$case_root" 1

case_root="$scratch/remove-pack-workflow"
seed "$case_root"
rm "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml"
run_case remove-pack-workflow-is-refused "$case_root" 1

case_root="$scratch/malformed-delegated"
seed "$case_root"
printf '\n{ [ }\n' >>"$case_root/.github/workflows/runner-spike-delegated.yml"
run_case malformed-delegated-is-refused "$case_root" 1

case_root="$scratch/malformed-pack"
seed "$case_root"
printf '\n{ [ }\n' >>"$case_root/.github/workflows/privileged-mcp-action-pack-release.yml"
run_case malformed-pack-is-refused "$case_root" 1

case_root="$scratch/malformed-foreign-workflow"
seed "$case_root"
printf '{ [ }\n' >"$case_root/.github/workflows/malformed-foreign.yml"
run_case malformed-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/provenance-is-not-attest"
seed "$case_root"
cat >"$case_root/.github/workflows/release-wrapper.yml" <<'YAML'
name: Wrapper is a different action
on: workflow_dispatch
jobs:
  provenance:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/attest-build-provenance@0f67c3f4856b2e3261c31976d6725780e5e4c373 # v4.1.1
YAML
run_case provenance-wrapper-is-not-a-callsite "$case_root" 0

case_root="$scratch/extra-callsite"
seed "$case_root"
cat >>"$case_root/.github/workflows/runner-spike-delegated.yml" <<YAML

# Mutation: a second active attest is outside the closed one-callsite contract.
uses: actions/attest@${OWNER_SHA} # v4.2.2
YAML
run_case extra-callsite-is-refused "$case_root" 1

case_root="$scratch/quoted-duplicate"
seed "$case_root"
cat >>"$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<YAML

# Mutation: quoted uses values are valid workflow YAML and remain active.
uses: "actions/attest@${OWNER_SHA}" # v4.2.2
YAML
run_case quoted-duplicate-is-refused "$case_root" 1

case_root="$scratch/foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/third-attest.yml" <<YAML
name: Mutated third actions/attest
jobs:
  attest:
    steps:
      - uses: actions/attest@${OWNER_SHA} # v4.2.2
YAML
run_case foreign-workflow-callsite-is-refused "$case_root" 1

case_root="$scratch/tag-pinned-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/tagged-attest.yml" <<'YAML'
name: Mutated tag-pinned actions/attest
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/attest@v4.2.2
YAML
run_case tag-pinned-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/alias-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/alias-attest.yml" <<'YAML'
name: Mutated alias actions/attest
on: workflow_dispatch
env:
  ATTEST_ACTION: &attest-action actions/attest@v4.2.2
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - uses: *attest-action
YAML
run_case alias-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/quoted-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/quoted-attest.yaml" <<YAML
name: Mutated quoted actions/attest
jobs:
  attest:
    steps:
      - uses: 'actions/attest@${OWNER_SHA}' # v4.2.2
YAML
run_case quoted-foreign-workflow-callsite-is-refused "$case_root" 1

case_root="$scratch/flow-mapping-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/flow-attest.yml" <<YAML
name: Mutated flow-mapping actions/attest
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - {uses: actions/attest@${OWNER_SHA}}
YAML
run_case flow-mapping-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/quoted-key-foreign-workflow"
seed "$case_root"
cat >"$case_root/.github/workflows/quoted-key-attest.yml" <<YAML
name: Mutated quoted-key actions/attest
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - "uses": actions/attest@${OWNER_SHA}
YAML
run_case quoted-key-foreign-workflow-is-refused "$case_root" 1

case_root="$scratch/hex-escaped-action"
seed "$case_root"
cat >"$case_root/.github/workflows/hex-escaped-attest.yml" <<YAML
name: Mutated hex-escaped actions/attest
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - uses: "actions/attest\\x40${OWNER_SHA}"
YAML
run_case hex-escaped-action-is-refused "$case_root" 1

case_root="$scratch/unicode-escaped-action"
seed "$case_root"
cat >"$case_root/.github/workflows/unicode-escaped-attest.yml" <<YAML
name: Mutated Unicode-escaped actions/attest
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - uses: "actions/attest\\u0040${OWNER_SHA}"
YAML
run_case unicode-escaped-action-is-refused "$case_root" 1

case_root="$scratch/case-variant-action"
seed "$case_root"
cat >"$case_root/.github/workflows/case-variant-attest.yml" <<YAML
name: Mutated case-variant actions/attest
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - uses: Actions/attest@${OWNER_SHA}
YAML
run_case case-variant-action-is-refused "$case_root" 1

case_root="$scratch/unicode-escaped-identity"
seed "$case_root"
cat >"$case_root/.github/workflows/unicode-identity-attest.yml" <<YAML
name: Mutated Unicode-escaped actions/attest identity
on: workflow_dispatch
jobs:
  attest:
    runs-on: ubuntu-latest
    steps:
      - uses: "\\u0061ctions/attest@${OWNER_SHA}"
YAML
run_case unicode-escaped-identity-is-refused "$case_root" 1

case_root="$scratch/escaped-line-break-action"
seed "$case_root"
python3 - "$case_root/.github/workflows/escaped-break-attest.yml" "$OWNER_SHA" <<'PY'
from pathlib import Path
import sys

sha = sys.argv[2]
Path(sys.argv[1]).write_text(
    "name: Mutated escaped-line-break actions/attest\n"
    "on: workflow_dispatch\n"
    "jobs:\n"
    "  attest:\n"
    "    runs-on: ubuntu-latest\n"
    "    steps:\n"
    f'      - uses: "actions/att\\\n          est@{sha}"\n',
    encoding="utf-8",
)
PY
run_case escaped-line-break-action-is-refused "$case_root" 1

case_root="$scratch/retarget-pack-sha256sum"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "          sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS" \
  "          sha256sum privileged-mcp-action-v0-clean-room.tar.gz > OTHERSUMS"
run_case retarget-pack-sha256sum-is-refused "$case_root" 1

case_root="$scratch/pack-producer-if-false"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        id: build-release-checksums
        shell: bash
        working-directory: release" \
  "        id: build-release-checksums
        if: false
        shell: bash
        working-directory: release"
run_case pack-producer-if-false-is-refused "$case_root" 1

case_root="$scratch/pack-producer-custom-shell"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        id: build-release-checksums
        shell: bash
        working-directory: release" \
  "        id: build-release-checksums
        shell: \"true {0}\"
        working-directory: release"
run_case pack-producer-custom-shell-is-refused "$case_root" 1

case_root="$scratch/pack-producer-extra-env"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        id: build-release-checksums
        shell: bash
        working-directory: release" \
  "        id: build-release-checksums
        shell: bash
        env:
          INERT: value
        working-directory: release"
run_case pack-producer-extra-env-is-refused "$case_root" 1

case_root="$scratch/pack-producer-never-called-function"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "          sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS" \
  "          make_checksums() {
            sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS
          }"
run_case pack-producer-never-called-function-is-refused "$case_root" 1

case_root="$scratch/delete-pack-sha256sum"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "          sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS
" \
  ""
run_case delete-pack-sha256sum-is-refused "$case_root" 1

case_root="$scratch/comment-only-pack-sha256sum"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "          sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS" \
  "          # sha256sum privileged-mcp-action-v0-clean-room.tar.gz > SHA256SUMS"
run_case comment-only-pack-sha256sum-is-refused "$case_root" 1

case_root="$scratch/retarget-delegated-proof-pack"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "          python3 scripts/ci/assay_runner_delegated_proof_pack.py \\" \
  "          python3 scripts/ci/other_delegated_proof_pack.py \\"
run_case retarget-delegated-proof-pack-is-refused "$case_root" 1

case_root="$scratch/delegated-producer-help"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "          python3 scripts/ci/assay_runner_delegated_proof_pack.py \\" \
  "          python3 scripts/ci/assay_runner_delegated_proof_pack.py --help \\"
run_case delegated-producer-help-is-refused "$case_root" 1

case_root="$scratch/delegated-producer-custom-shell"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        id: build-proof-pack
        if: always()
        shell: bash" \
  "        id: build-proof-pack
        if: always()
        shell: \"true {0}\""
run_case delegated-producer-custom-shell-is-refused "$case_root" 1

case_root="$scratch/delegated-producer-extra-env"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        id: build-proof-pack
        if: always()
        shell: bash" \
  "        id: build-proof-pack
        if: always()
        shell: bash
        env:
          ASSAY_RUNNER_DELEGATED_PROOF_ROOT: /tmp"
run_case delegated-producer-extra-env-is-refused "$case_root" 1

case_root="$scratch/delegated-job-root-env-retarget"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  '      ASSAY_RUNNER_DELEGATED_PROOF_ROOT: /tmp/assay-runner-proof-${{ github.run_id }}' \
  '      ASSAY_RUNNER_DELEGATED_PROOF_ROOT: /tmp'
run_case delegated-job-root-env-retarget-is-refused "$case_root" 1

case_root="$scratch/delegated-job-upload-env-retarget"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  '      ASSAY_RUNNER_DELEGATED_PROOF_UPLOAD: ${{ github.workspace }}/assay-runner-proof-upload' \
  '      ASSAY_RUNNER_DELEGATED_PROOF_UPLOAD: /tmp'
run_case delegated-job-upload-env-retarget-is-refused "$case_root" 1

case_root="$scratch/delete-delegated-proof-pack"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "          python3 scripts/ci/assay_runner_delegated_proof_pack.py \\" \
  "          true \\"
run_case delete-delegated-proof-pack-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-comment"
seed "$case_root"
python3 - "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """      - name: Retain attestation bundle
        id: retain-release-attestation
        shell: bash
        working-directory: ${{ github.workspace }}
        env:
          ATTESTATION_BUNDLE: ${{ steps.attest.outputs.bundle-path }}
        run: |
          set -euo pipefail
          test -n "$ATTESTATION_BUNDLE"
          cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json
"""
new = """      # Retain attestation bundle via steps.attest.outputs.bundle-path
"""
if text.count(old) != 1:
    raise SystemExit("pack-consumer-comment subject missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
run_case pack-consumer-comment-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-inert"
seed "$case_root"
python3 - "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """        run: |
          set -euo pipefail
          test -n "$ATTESTATION_BUNDLE"
          cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json
"""
new = """        run: echo inert
"""
if text.count(old) != 1:
    raise SystemExit("pack-consumer-inert subject missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
run_case pack-consumer-inert-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-custom-shell"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        id: retain-release-attestation
        shell: bash
        working-directory: \${{ github.workspace }}
        env:" \
  "        id: retain-release-attestation
        shell: \"true {0}\"
        working-directory: \${{ github.workspace }}
        env:"
run_case pack-consumer-custom-shell-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-cwd-retarget"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        id: retain-release-attestation
        shell: bash
        working-directory: \${{ github.workspace }}" \
  "        id: retain-release-attestation
        shell: bash
        working-directory: /tmp"
run_case pack-consumer-cwd-retarget-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-extra-env"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  '          ATTESTATION_BUNDLE: ${{ steps.attest.outputs.bundle-path }}' \
  '          ATTESTATION_BUNDLE: ${{ steps.attest.outputs.bundle-path }}
          INERT: value'
run_case pack-consumer-extra-env-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-if-false"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  '          cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json' \
  '          if false; then
            cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json
          fi'
run_case pack-consumer-if-false-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-never-called-function"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  '          cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json' \
  '          retain_bundle() {
            cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json
          }'
run_case pack-consumer-never-called-function-is-refused "$case_root" 1

case_root="$scratch/delegated-consumer-scalar"
seed "$case_root"
python3 - "$case_root/.github/workflows/runner-spike-delegated.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """      - name: Retain delegated proof attestation bundle
        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: bash
        working-directory: ${{ github.workspace }}
        run: |
          set -euo pipefail
          cp "${{ steps.attest-proof-pack.outputs.bundle-path }}" \\
            "$ASSAY_RUNNER_DELEGATED_PROOF_UPLOAD/attestation-bundle.json"
"""
new = """      - name: Retain delegated proof attestation bundle
        NOTE: steps.attest-proof-pack.outputs.bundle-path
"""
if text.count(old) != 1:
    raise SystemExit("delegated-consumer-scalar subject missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
run_case delegated-consumer-scalar-is-refused "$case_root" 1

case_root="$scratch/delegated-consumer-custom-shell"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: bash" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: \"true {0}\""
run_case delegated-consumer-custom-shell-is-refused "$case_root" 1

case_root="$scratch/delegated-consumer-cwd-retarget"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: bash
        working-directory: \${{ github.workspace }}" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: bash
        working-directory: /tmp"
run_case delegated-consumer-cwd-retarget-is-refused "$case_root" 1

case_root="$scratch/delegated-consumer-step-env-retarget"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: bash" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''
        shell: bash
        env:
          ASSAY_RUNNER_DELEGATED_PROOF_UPLOAD: /tmp"
run_case delegated-consumer-step-env-retarget-is-refused "$case_root" 1

case_root="$scratch/pack-attest-if-false"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" \
  "        id: attest
        uses: actions/attest@${OWNER_SHA} # v4.2.2" \
  "        id: attest
        if: false
        uses: actions/attest@${OWNER_SHA} # v4.2.2"
run_case pack-attest-if-false-is-refused "$case_root" 1

case_root="$scratch/pack-attest-if-compound-false"
seed "$case_root"
python3 - "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """        id: attest
        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2
"""
new = """        id: attest
        if: ${{ false && always() }}
        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2
"""
if text.count(old) != 1:
    raise SystemExit("compound-false attest subject missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
run_case pack-attest-if-compound-false-is-refused "$case_root" 1

case_root="$scratch/pack-attest-if-unsupported"
seed "$case_root"
python3 - "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
old = """        id: attest
        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2
"""
new = """        id: attest
        if: ${{ github.ref == 'refs/heads/main' }}
        uses: actions/attest@1e69f48acb82d1966a394da916b4c1698aa569d6 # v4.2.2
"""
if text.count(old) != 1:
    raise SystemExit("unsupported attest condition subject missing")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
run_case pack-attest-if-unsupported-is-refused "$case_root" 1

case_root="$scratch/delegated-consumer-if-false"
seed "$case_root"
mutate_once \
  "$case_root/.github/workflows/runner-spike-delegated.yml" \
  "        id: retain-proof-attestation
        if: always() && steps.attest-proof-pack.outputs.bundle-path != ''" \
  "        id: retain-proof-attestation
        if: false"
run_case delegated-consumer-if-false-is-refused "$case_root" 1

case_root="$scratch/pack-consumer-other-job"
seed "$case_root"
python3 - "$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<'PY'
from pathlib import Path
import sys

path = Path(sys.argv[1])
text = path.read_text(encoding="utf-8")
consumer = """      - name: Retain attestation bundle
        id: retain-release-attestation
        shell: bash
        working-directory: ${{ github.workspace }}
        env:
          ATTESTATION_BUNDLE: ${{ steps.attest.outputs.bundle-path }}
        run: |
          set -euo pipefail
          test -n "$ATTESTATION_BUNDLE"
          cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json

"""
if text.count(consumer) != 1:
    raise SystemExit("pack-consumer-other-job subject missing")
text = text.replace(consumer, "", 1)
text += """
  retain-elsewhere:
    runs-on: ubuntu-24.04
    steps:
      - name: Retain attestation bundle
        id: retain-release-attestation
        env:
          ATTESTATION_BUNDLE: ${{ steps.attest.outputs.bundle-path }}
        run: |
          set -euo pipefail
          test -n "$ATTESTATION_BUNDLE"
          cp "$ATTESTATION_BUNDLE" release/attestation-bundle.json
"""
path.write_text(text, encoding="utf-8")
PY
run_case pack-consumer-other-job-is-refused "$case_root" 1

case_root="$scratch/inert-note-census"
seed "$case_root"
cat >>"$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<YAML

NOTE: "actions/attest@${OWNER_SHA}"
YAML
run_case inert-note-attest-is-not-a-callsite "$case_root" 0

case_root="$scratch/comment-only-control"
seed "$case_root"
cat >>"$case_root/.github/workflows/runner-spike-delegated.yml" <<'YAML'

# No-op mutation control: executable producer and consumer steps are unchanged.
YAML
run_case comment-only-control-is-green "$case_root" 0

case_root="$scratch/env-comment-only-control"
seed "$case_root"
cat >>"$case_root/.github/workflows/runner-spike-delegated.yml" <<'YAML'

# ASSAY_RUNNER_DELEGATED_PROOF_UPLOAD remains owned by the delegated job env.
YAML
run_case env-comment-only-control-is-green "$case_root" 0

case_root="$scratch/cwd-comment-only-control"
seed "$case_root"
cat >>"$case_root/.github/workflows/privileged-mcp-action-pack-release.yml" <<'YAML'

# Retention commands remain explicitly workspace-bound.
YAML
run_case cwd-comment-only-control-is-green "$case_root" 0

case_root="$scratch/missing-ruby"
seed "$case_root"
norb="$scratch/norb/bin"
mkdir -p "$norb"
ln -sfn "$(command -v python3)" "$norb/python3"
if PATH="$norb" command -v ruby >/dev/null 2>&1; then
  echo "FAIL: ruby-free PATH still locates ruby" >&2
  exit 1
fi
status=0
(cd "$case_root" && PATH="$norb" python3 "$CHECKER") >"$scratch/missing-ruby.log" 2>&1 || status=$?
if [[ "$status" -ne 1 ]]; then
  cat "$scratch/missing-ruby.log" >&2
  echo "FAIL: missing-ruby exited $status, wanted 1" >&2
  exit 1
fi
if ! grep -q "yaml parser unavailable" "$scratch/missing-ruby.log"; then
  cat "$scratch/missing-ruby.log" >&2
  echo "FAIL: missing-ruby did not fail-closed on parser unavailability" >&2
  exit 1
fi
echo "ok    missing-ruby-is-refused (exit $status)"

printf 'PASS: actions/attest lockstep battery\n'
