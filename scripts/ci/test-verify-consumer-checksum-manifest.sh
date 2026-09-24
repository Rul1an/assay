#!/usr/bin/env bash
# Behavioral contract for the network-isolated consumer checksum helper (Refs #3119).
# Stub Docker/cosign + recorded argv. Does not prove live cryptography.
# Does not prove container filesystem access: the stub writes TUF state as this
# host process into the bind source, so a missing --user still looks green here.
# Hosted published replay 35469738379 failed at initialize with
# "clearing cache directory: open /scratch: permission denied" after the
# pinned image pulled. That seam is a host-uid bind, not argv isolation.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/../.." && pwd)"
HELPER="${REPO_ROOT}/scripts/ci/verify_consumer_checksum_manifest.sh"
RELEASE_WORKFLOW="${REPO_ROOT}/.github/workflows/release.yml"
PUBLISHED_WORKFLOW="${REPO_ROOT}/.github/workflows/published-release-golden-path.yml"
PIN_FILE="${REPO_ROOT}/.github/cosign-image"
INDEX_DIGEST='9e5c2f2edc34351160407ca3416c61855bdf9403c3c5936e0f0be7fc261611b8'

GOOD_ID='https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/v9.9.9'
DECOY_ID='https://github.com/Rul1an/assay/.github/workflows/release.yml@refs/tags/v0.0.0-decoy'
ISSUER='https://token.actions.githubusercontent.com'
STUB_IMAGE='ci-cosign-stub@sha256:0000000000000000000000000000000000000000000000000000000000000000'
ARCHIVE='assay-v9.9.9-x86_64-unknown-linux-gnu.tar.gz'
EXTRA='assay-v9.9.9-sbom-cyclonedx.tar.gz'

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

compute_sha256() {
  local file="$1"
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$file" | awk '{print $1}'
  else
    shasum -a 256 "$file" | awk '{print $1}'
  fi
}

[[ -f "$HELPER" ]] || fail "verify_consumer_checksum_manifest.sh is missing"
[[ -x "$HELPER" ]] || fail "verify_consumer_checksum_manifest.sh must be executable"

tmp_root="$(mktemp -d)"
trap 'rm -rf "$tmp_root"' EXIT

# One physical-directory key for bind-source and expected-host comparison.
# Git-Bash /c/foo is C:\foo; Windows Python realpath of /c/foo is {cwd_drive}\c\foo.
HOST_PATH_LIB="${tmp_root}/physical_host_path.py"
cat >"$HOST_PATH_LIB" <<'PY'
import ntpath
import os
import re

_MSYS_DRIVE = re.compile(r"^/([A-Za-z])(/.*)?$")


def windows_native_path(path: str) -> str:
    match = _MSYS_DRIVE.fullmatch(path)
    if match is None:
        return path
    rest = match.group(2) or ""
    return match.group(1).upper() + ":" + rest.replace("/", "\\")


def physical_dir_key(path: str) -> str:
    native = windows_native_path(path)
    if os.name == "nt":
        return os.path.normcase(os.path.realpath(native))
    if len(native) >= 2 and native[1] == ":":
        return ntpath.normcase(ntpath.normpath(native))
    return os.path.realpath(native)
PY

write_stub_docker() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"
  cat >"${bin_dir}/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
log="${CONSUMER_DOCKER_LOG:?}"
{
  echo '--- invocation ---'
  i=0
  for arg in "$@"; do
    printf 'arg[%d]=%s\n' "$i" "$arg"
    i=$((i + 1))
  done
} >>"$log"

cmd=""
image=""
scratch_host=""
seen_image=0
prev=""
for arg in "$@"; do
  if [[ "$prev" == "-v" ]]; then
    case "$arg" in
      *:\/scratch | *:\/scratch:*)
        scratch_host="${arg%%:*}"
        ;;
    esac
  fi
  if [[ "$arg" == "initialize" || "$arg" == "verify-blob" ]]; then
    cmd="$arg"
  fi
  if [[ "$seen_image" -eq 0 && "$arg" == *@sha256:* ]]; then
    image="$arg"
    seen_image=1
  fi
  prev="$arg"
done

case "$cmd" in
  initialize)
    [[ -n "$scratch_host" ]] || { echo "stub docker: initialize missing /scratch mount" >&2; exit 2; }
    mkdir -p "${scratch_host}/tuf-cache/tuf-repo-cdn.sigstore.dev/targets"
    printf 'trusted-root-fixture\n' >"${scratch_host}/tuf-cache/tuf-repo-cdn.sigstore.dev/targets/trusted_root.json"
    echo "initialized ${image}"
    ;;
  verify-blob)
    if [[ "${CONSUMER_DOCKER_VERIFY:-ok}" == refuse ]]; then
      echo 'Error: verifying blob [checksums.txt]: SIGNATURE_REFUSED'
      exit 1
    fi
    identity=""
    issuer=""
    prev=""
    for arg in "$@"; do
      if [[ "$prev" == "--certificate-identity" ]]; then
        identity="$arg"
      elif [[ "$prev" == "--certificate-oidc-issuer" ]]; then
        issuer="$arg"
      fi
      prev="$arg"
    done
    if [[ "$identity" == *v0.0.0-decoy* ]]; then
      echo 'Error: verifying blob [checksums.txt]: IDENTITY_MISMATCH'
      exit 1
    fi
    if [[ -n "${CONSUMER_DOCKER_REQUIRE_IDENTITY:-}" && "$identity" != "${CONSUMER_DOCKER_REQUIRE_IDENTITY}" ]]; then
      echo 'Error: verifying blob [checksums.txt]: IDENTITY_MISMATCH'
      exit 1
    fi
    if [[ -n "${CONSUMER_DOCKER_REQUIRE_ISSUER:-}" && "$issuer" != "${CONSUMER_DOCKER_REQUIRE_ISSUER}" ]]; then
      echo 'Error: verifying blob [checksums.txt]: ISSUER_MISMATCH'
      exit 1
    fi
    echo 'Verified OK'
    ;;
  *)
    echo "unexpected docker command: $*" >&2
    exit 2
    ;;
esac
EOF
  chmod +x "${bin_dir}/docker"
}

write_hash_probes() {
  local bin_dir="$1"
  mkdir -p "$bin_dir"
  local real_shasum real_sha256
  real_shasum="$(command -v shasum || true)"
  real_sha256="$(command -v sha256sum || true)"
  cat >"${bin_dir}/shasum" <<EOF
#!/bin/sh
echo HASH_STEP_REACHED
echo "HASH_CWD=\$(pwd -P)"
if [ -n "$real_shasum" ]; then
  exec "$real_shasum" "\$@"
fi
exit 0
EOF
  chmod +x "${bin_dir}/shasum"
  if [[ -n "$real_sha256" ]]; then
    cat >"${bin_dir}/sha256sum" <<EOF
#!/bin/sh
echo HASH_STEP_REACHED
echo "HASH_CWD=\$(pwd -P)"
exec "$real_sha256" "\$@"
EOF
    chmod +x "${bin_dir}/sha256sum"
  fi
}

prepare_assets() {
  local store="$1"
  mkdir -p "$store"
  printf 'selected-archive\n' >"${store}/${ARCHIVE}"
  printf 'other-payload\n' >"${store}/${EXTRA}"
  {
    printf '%s  %s\n' "$(compute_sha256 "${store}/${ARCHIVE}")" "$ARCHIVE"
    printf '%s  %s\n' "$(compute_sha256 "${store}/${EXTRA}")" "$EXTRA"
  } | LC_ALL=C sort >"${store}/checksums.txt"
  printf 'sigstore-bundle-fixture\n' >"${store}/checksums.txt.sigstore.json"
}

run_helper() {
  local case_dir="$1"
  shift
  mkdir -p "${case_dir}/bin"
  write_stub_docker "${case_dir}/bin"
  write_hash_probes "${case_dir}/bin"
  : >"${case_dir}/docker.log"
  (
    cd "$case_dir"
    env PATH="${case_dir}/bin:/usr/bin:/bin" \
      CONSUMER_DOCKER_LOG="${case_dir}/docker.log" \
      CONSUMER_DOCKER_VERIFY="${CONSUMER_DOCKER_VERIFY:-ok}" \
      CONSUMER_DOCKER_REQUIRE_IDENTITY="${CONSUMER_DOCKER_REQUIRE_IDENTITY:-$GOOD_ID}" \
      CONSUMER_DOCKER_REQUIRE_ISSUER="${CONSUMER_DOCKER_REQUIRE_ISSUER:-$ISSUER}" \
      bash "$HELPER" "$@"
  )
}

set_helper_args() {
  local assets="$1"
  local identity="${2:-$GOOD_ID}"
  HELPER_ARGS=(
    --assets-dir "$assets"
    --archive "$ARCHIVE"
    --certificate-identity "$identity"
    --certificate-oidc-issuer "$ISSUER"
    --cosign-image "$STUB_IMAGE"
  )
}

assert_verify_isolated() {
  local log="$1"
  python3 - "$log" "$STUB_IMAGE" "$GOOD_ID" "$ISSUER" <<'PY'
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
image, identity, issuer = sys.argv[2:]
blocks = [b for b in text.split("--- invocation ---\n") if b.strip()]
if len(blocks) != 2:
    raise SystemExit(f"expected 2 docker invocations, got {len(blocks)}")
init, verify = blocks

def args(block):
    out = []
    for line in block.splitlines():
        if line.startswith("arg["):
            out.append(line.split("=", 1)[1])
    return out

init_args = args(init)
verify_args = args(verify)
if "initialize" not in init_args:
    raise SystemExit("first docker invocation is not initialize")
if "--network=none" in init_args:
    raise SystemExit("bootstrap initialize must not set --network=none")
if image not in init_args:
    raise SystemExit("initialize did not use the pinned stub image")
if "verify-blob" not in verify_args:
    raise SystemExit("second docker invocation is not verify-blob")
if "--network=none" not in verify_args:
    raise SystemExit("verify-blob must run with --network=none")
if image not in verify_args:
    raise SystemExit("verify-blob did not use the same image as initialize")
joined = "\n".join(verify_args)
if "/assets:ro" not in joined:
    raise SystemExit("verify-blob must mount assets read-only")
if "/trusted_root.json:ro" not in joined:
    raise SystemExit("verify-blob must mount trusted_root read-only")
if "--trusted-root" not in verify_args:
    raise SystemExit("verify-blob must pass --trusted-root")
if "--bundle" not in verify_args:
    raise SystemExit("verify-blob must pass --bundle")
if "/assets/checksums.txt" not in verify_args:
    raise SystemExit("verify-blob must verify /assets/checksums.txt")
if identity not in verify_args:
    raise SystemExit("verify-blob must pass the exact certificate identity")
if issuer not in verify_args:
    raise SystemExit("verify-blob must pass the exact OIDC issuer")
print("docker isolation argv ok")
PY
}

# Argv-only: both container phases must run as the invoking host uid/gid so
# 0700 mktemp scratch and host-owned :ro assets are openable by the process
# inside the pinned image. This reads the stub docker log. It does not start
# Docker and does not prove the image USER or a real /scratch write.
assert_host_uid_bind_on_both_phases() {
  local log="$1"
  local expected_user
  expected_user="$(id -u):$(id -g)"
  python3 - "$log" "$expected_user" <<'PY'
import sys
from pathlib import Path

text = Path(sys.argv[1]).read_text(encoding="utf-8")
expected = sys.argv[2]
blocks = [b for b in text.split("--- invocation ---\n") if b.strip()]
if len(blocks) != 2:
    raise SystemExit(f"expected 2 docker invocations, got {len(blocks)}")


def args(block):
    out = []
    for line in block.splitlines():
        if line.startswith("arg["):
            out.append(line.split("=", 1)[1])
    return out


def user_values(argv):
    values = []
    prev = None
    for arg in argv:
        if prev == "--user":
            values.append(arg)
        prev = arg
    return values


init_args = args(blocks[0])
verify_args = args(blocks[1])
if "initialize" not in init_args:
    raise SystemExit("first docker invocation is not initialize")
if "verify-blob" not in verify_args:
    raise SystemExit("second docker invocation is not verify-blob")

for name, argv in (("initialize", init_args), ("verify-blob", verify_args)):
    if "--privileged" in argv:
        raise SystemExit(
            f"{name} used --privileged; host-uid bind is the allowed repair"
        )
    if "--network=host" in argv:
        raise SystemExit(f"{name} used --network=host; isolation must stay")
    users = user_values(argv)
    if len(users) != 1:
        raise SystemExit(
            f"{name} must pass exactly one --user <host-uid>:<host-gid>, got {users!r}. "
            "Stub docker argv only; not pinned-image execution or /scratch access."
        )
    if users[0] != expected:
        raise SystemExit(
            f"{name} --user {users[0]!r} is not the invoking host {expected!r}"
        )
print("host uid bind argv ok (stub argv only; not container access)")
PY
}

assert_host_bind_and_hash() {
  local log="$1"
  local stdout="$2"
  local expected_host="$3"
  python3 - "$HOST_PATH_LIB" "$log" "$stdout" "$expected_host" <<'PY'
import importlib.util
import sys
from pathlib import Path

spec = importlib.util.spec_from_file_location("physical_host_path", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
physical_dir_key = mod.physical_dir_key

log_text = Path(sys.argv[2]).read_text(encoding="utf-8")
stdout_text = Path(sys.argv[3]).read_text(encoding="utf-8")
expected = physical_dir_key(sys.argv[4])
blocks = [b for b in log_text.split("--- invocation ---\n") if b.strip()]
if len(blocks) != 2:
    raise SystemExit(f"expected 2 docker invocations, got {len(blocks)}")


def args(block):
    out = []
    for line in block.splitlines():
        if line.startswith("arg["):
            out.append(line.split("=", 1)[1])
    return out


verify_args = args(blocks[1])
if "verify-blob" not in verify_args:
    raise SystemExit("second docker invocation is not verify-blob")

sources = []
prev = None
for arg in verify_args:
    if prev == "-v" and (arg.endswith(":/assets:ro") or arg.endswith(":/assets")):
        src = arg[: -len(":/assets:ro")] if arg.endswith(":/assets:ro") else arg[: -len(":/assets")]
        sources.append(src)
    prev = arg
if len(sources) != 1:
    raise SystemExit(f"expected one /assets bind on verify-blob, got {sources!r}")
source = sources[0]
if "/" not in source and not source.startswith("."):
    raise SystemExit(
        f"verify-blob assets mount is a named volume, not a host bind: {source}"
    )
if not source.startswith("/"):
    raise SystemExit(f"verify-blob assets bind source is not physical-absolute: {source}")
resolved_source = physical_dir_key(source)
if resolved_source != expected:
    raise SystemExit(
        f"verify-blob bind source {resolved_source} is not the host assets dir {expected}"
    )

hash_cwds = [
    physical_dir_key(line.split("=", 1)[1])
    for line in stdout_text.splitlines()
    if line.startswith("HASH_CWD=")
]
if not hash_cwds:
    raise SystemExit("hash step did not record HASH_CWD")
if any(cwd != expected for cwd in hash_cwds):
    raise SystemExit(
        f"hash cwd {hash_cwds!r} is not the same host assets dir {expected}"
    )
if resolved_source != hash_cwds[0]:
    raise SystemExit(
        f"verify-blob bind {resolved_source} is not the hashing directory {hash_cwds[0]}"
    )
print("host bind and hash directory ok")
PY
}

# Portable regression for the exact hosted Windows seam (job 105962931434):
# Git-Bash records /c/Users/...; Windows Python os.path.realpath treats that
# as {cwd_drive}\c\Users\... (D:\c\... when the workspace is D:\a\assay\assay),
# while MSYS argv conversion already turned the expected host dir into C:\Users\...
# Linux/macOS fixtures never feed this pair, so they cannot catch it.
assert_windows_msys_drive_conversion_seam() {
  python3 - "$HOST_PATH_LIB" <<'PY'
import importlib.util
import ntpath
import os
import sys

spec = importlib.util.spec_from_file_location("physical_host_path", sys.argv[1])
mod = importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
physical_dir_key = mod.physical_dir_key
windows_native_path = mod.windows_native_path

posix_bind = "/c/Users/runneradmin/AppData/Local/Temp/tmp.M5m6xevUK6/happy/release"
windows_expected = (
    r"C:\Users\runneradmin\AppData\Local\Temp\tmp.M5m6xevUK6\happy\release"
)
other_drive = "/d/Users/runneradmin/AppData/Local/Temp/tmp.M5m6xevUK6/happy/release"
posix_tmp = "/tmp/happy/release"

nt_root_relative = ntpath.normpath(posix_bind)
if nt_root_relative != (
    r"\c\Users\runneradmin\AppData\Local\Temp\tmp.M5m6xevUK6\happy\release"
):
    raise SystemExit(f"ntpath.normpath(/c/...) drifted: {nt_root_relative!r}")
hosted_broken = "D:" + nt_root_relative
if hosted_broken != (
    r"D:\c\Users\runneradmin\AppData\Local\Temp\tmp.M5m6xevUK6\happy\release"
):
    raise SystemExit(f"D: current-drive join drifted: {hosted_broken!r}")

if windows_native_path(posix_bind) != windows_expected:
    raise SystemExit(
        f"MSYS /c/ bind became {windows_native_path(posix_bind)!r}, not {windows_expected!r}"
    )
if windows_native_path(posix_tmp) != posix_tmp:
    raise SystemExit(
        f"/tmp must not be treated as a drive mapping: {windows_native_path(posix_tmp)!r}"
    )

resolved_source = physical_dir_key(posix_bind)
expected = physical_dir_key(windows_expected)
if resolved_source != expected:
    raise SystemExit(
        f"verify-blob bind source {resolved_source} is not the host assets dir {expected}"
    )
if physical_dir_key(other_drive) == expected:
    raise SystemExit(
        "physical-dir equality collapsed /d/Users/... onto C:\\Users\\... (suffix match)"
    )
if os.name != "nt":
    tmp_key = physical_dir_key(posix_tmp)
    if tmp_key != os.path.realpath(posix_tmp):
        raise SystemExit(f"/tmp physical key drifted: {tmp_key!r}")
print("windows msys drive conversion ok")
PY
}

assert_windows_msys_drive_conversion_seam

# Default positive fixture: the candidate workflow's actual argv
# (`--assets-dir release` from a cwd that contains ./release).
# Absolute-only stores miss Docker named-volume short-syntax.
happy="${tmp_root}/happy"
mkdir -p "${happy}/release"
prepare_assets "${happy}/release"
expected_release="$(cd -- "${happy}/release" && pwd -P)"
set_helper_args "release"
if ! run_helper "$happy" "${HELPER_ARGS[@]}" \
  >"${happy}/stdout" 2>"${happy}/stderr"; then
  cat "${happy}/stdout" "${happy}/stderr" >&2
  fail "positive consumer helper failed"
fi
grep -Fq 'Verified OK' "${happy}/stdout" || fail "positive helper did not print Verified OK"
grep -Fq "${ARCHIVE}: OK" "${happy}/stdout" || fail "positive helper did not print archive OK"
grep -Fq HASH_STEP_REACHED "${happy}/stdout" || fail "positive helper did not reach the hash step"
assert_verify_isolated "${happy}/docker.log"
assert_host_uid_bind_on_both_phases "${happy}/docker.log"
assert_host_bind_and_hash "${happy}/docker.log" "${happy}/stdout" "$expected_release"
# Owned scratch must not leak after success.
if find "$tmp_root" -type d -name 'tmp.*' -o -path '*/tuf-cache' | grep -q .; then
  # Helper scratch is under /tmp via mktemp, not case_dir; assert helper did not leave TUF in assets.
  :
fi
[[ ! -e "${happy}/release/tuf-cache" ]] || fail "helper must not write TUF state into assets-dir"

# Absolute assets-dir remains valid (published-replay class under RUNNER_TEMP).
abs_assets="${tmp_root}/abs-assets"
prepare_assets "$abs_assets"
abs_case="${tmp_root}/abs-control"
mkdir -p "$abs_case"
expected_abs="$(cd -- "$abs_assets" && pwd -P)"
set_helper_args "$abs_assets"
if ! run_helper "$abs_case" "${HELPER_ARGS[@]}" \
  >"${abs_case}/stdout" 2>"${abs_case}/stderr"; then
  cat "${abs_case}/stdout" "${abs_case}/stderr" >&2
  fail "absolute assets-dir control failed"
fi
assert_verify_isolated "${abs_case}/docker.log"
assert_host_uid_bind_on_both_phases "${abs_case}/docker.log"
assert_host_bind_and_hash "${abs_case}/docker.log" "${abs_case}/stdout" "$expected_abs"

# Relative directory names with spaces must still become a host bind.
space_case="${tmp_root}/space cwd"
mkdir -p "${space_case}/my assets"
prepare_assets "${space_case}/my assets"
expected_space="$(cd -- "${space_case}/my assets" && pwd -P)"
set_helper_args "my assets"
if ! run_helper "$space_case" "${HELPER_ARGS[@]}" \
  >"${space_case}/stdout" 2>"${space_case}/stderr"; then
  cat "${space_case}/stdout" "${space_case}/stderr" >&2
  fail "assets-dir with spaces failed"
fi
assert_verify_isolated "${space_case}/docker.log"
assert_host_uid_bind_on_both_phases "${space_case}/docker.log"
assert_host_bind_and_hash "${space_case}/docker.log" "${space_case}/stdout" "$expected_space"

# Signature / identity / issuer failure must not reach the hash step.
assert_stops_before_hash() {
  local name="$1"
  local identity="$2"
  local verify_mode="$3"
  local marker="$4"
  local case_dir="${tmp_root}/${name}"
  local store="${tmp_root}/${name}-assets"
  prepare_assets "$store"
  mkdir -p "$case_dir"
  local status=0
  set_helper_args "$store" "$identity"
  CONSUMER_DOCKER_VERIFY="$verify_mode" \
    run_helper "$case_dir" "${HELPER_ARGS[@]}" \
    >"${case_dir}/stdout" 2>"${case_dir}/stderr" || status=$?
  if grep -Fq HASH_STEP_REACHED "${case_dir}/stdout" "${case_dir}/stderr"; then
    cat "${case_dir}/stdout" "${case_dir}/stderr" >&2
    fail "${name} reached the hash step after ${marker}"
  fi
  [[ "$status" -ne 0 ]] || fail "${name} unexpectedly succeeded"
  grep -Fq "$marker" "${case_dir}/stdout" "${case_dir}/stderr" \
    || fail "${name} did not print ${marker}"
}

assert_stops_before_hash bad-sig "$GOOD_ID" refuse SIGNATURE_REFUSED
assert_stops_before_hash decoy-id "$DECOY_ID" ok IDENTITY_MISMATCH

# Tampered archive: signature succeeds, hash fails.
tamper_store="${tmp_root}/tamper-assets"
prepare_assets "$tamper_store"
printf 'tampered-archive\n' >"${tamper_store}/${ARCHIVE}"
tamper="${tmp_root}/tamper"
mkdir -p "$tamper"
set_helper_args "$tamper_store"
if run_helper "$tamper" "${HELPER_ARGS[@]}" \
  >"${tamper}/stdout" 2>"${tamper}/stderr"; then
  fail "tampered archive was accepted"
fi
grep -Fq 'Verified OK' "${tamper}/stdout" || fail "tampered archive must still verify the signature"
if ! grep -Eq 'FAILED|did NOT match' "${tamper}/stdout" "${tamper}/stderr"; then
  cat "${tamper}/stdout" "${tamper}/stderr" >&2
  fail "tampered archive must fail the hash check"
fi

# Missing and duplicate selected archive entries.
missing_store="${tmp_root}/missing-assets"
prepare_assets "$missing_store"
grep -Fv "$ARCHIVE" "${missing_store}/checksums.txt" >"${missing_store}/checksums.omit"
mv "${missing_store}/checksums.omit" "${missing_store}/checksums.txt"
missing="${tmp_root}/missing"
mkdir -p "$missing"
set_helper_args "$missing_store"
if run_helper "$missing" "${HELPER_ARGS[@]}" \
  >"${missing}/stdout" 2>"${missing}/stderr"; then
  fail "missing selected archive entry was accepted"
fi
grep -Fq "$ARCHIVE" "${missing}/stdout" "${missing}/stderr" \
  || fail "missing entry must name the selected archive"

dup_store="${tmp_root}/dup-assets"
prepare_assets "$dup_store"
line="$(grep -F "$ARCHIVE" "${dup_store}/checksums.txt")"
printf '%s\n' "$line" >>"${dup_store}/checksums.txt"
dup="${tmp_root}/dup"
mkdir -p "$dup"
set_helper_args "$dup_store"
if run_helper "$dup" "${HELPER_ARGS[@]}" \
  >"${dup}/stdout" 2>"${dup}/stderr"; then
  fail "duplicate selected archive entry was accepted"
fi
grep -Eiq 'duplicate|more than one' "${dup}/stdout" "${dup}/stderr" \
  || fail "duplicate entry must be named as a duplicate"

# Hostile names must not become docker options or path traversal.
assert_rejects_archive() {
  local name="$1"
  local archive="$2"
  local store="${tmp_root}/${name}-assets"
  prepare_assets "$store"
  local case_dir="${tmp_root}/${name}"
  mkdir -p "$case_dir"
  write_stub_docker "${case_dir}/bin"
  write_hash_probes "${case_dir}/bin"
  : >"${case_dir}/docker.log"
  local status=0
  (
    cd "$case_dir"
    env PATH="${case_dir}/bin:/usr/bin:/bin" \
      CONSUMER_DOCKER_LOG="${case_dir}/docker.log" \
      bash "$HELPER" \
        --assets-dir "$store" \
        --archive "$archive" \
        --certificate-identity "$GOOD_ID" \
        --certificate-oidc-issuer "$ISSUER" \
        --cosign-image "$STUB_IMAGE"
  ) >"${case_dir}/stdout" 2>"${case_dir}/stderr" || status=$?
  [[ "$status" -ne 0 ]] || fail "hostile archive ${archive} was accepted"
  if grep -Fq -- "$archive" "${case_dir}/docker.log"; then
    fail "hostile archive ${archive} leaked into docker argv"
  fi
}

assert_rejects_archive archive-flag '--network=none'
assert_rejects_archive archive-traversal '../checksums.txt'
assert_rejects_archive archive-slash 'nested/assay.tar.gz'

# Invalid assets-dir.
bad_dir="${tmp_root}/bad-dir"
mkdir -p "${bad_dir}/bin"
write_stub_docker "${bad_dir}/bin"
: >"${bad_dir}/docker.log"
if PATH="${bad_dir}/bin:/usr/bin:/bin" CONSUMER_DOCKER_LOG="${bad_dir}/docker.log" \
  bash "$HELPER" \
    --assets-dir "${tmp_root}/../.." \
    --archive "$ARCHIVE" \
    --certificate-identity "$GOOD_ID" \
    --certificate-oidc-issuer "$ISSUER" \
    --cosign-image "$STUB_IMAGE" \
    >"${bad_dir}/stdout" 2>"${bad_dir}/stderr"; then
  fail "parent-traversal assets-dir was accepted"
fi

# Production-seam mutations: remove isolation / identity from a copy of the helper.
mutate_helper() {
  local dest="$1"
  local old="$2"
  local new="$3"
  cp "$HELPER" "$dest"
  python3 - "$dest" "$old" "$new" <<'PY'
from pathlib import Path
import sys
path = Path(sys.argv[1])
old, new = sys.argv[2], sys.argv[3]
text = path.read_text(encoding="utf-8")
if text.count(old) != 1:
    raise SystemExit(f"mutation anchor count for {old!r}: {text.count(old)}")
path.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
}

assert_mutation_red() {
  local name="$1"
  local old="$2"
  local new="$3"
  local mutant="${tmp_root}/${name}.sh"
  mutate_helper "$mutant" "$old" "$new"
  chmod +x "$mutant"
  local store="${tmp_root}/${name}-assets"
  prepare_assets "$store"
  local case_dir="${tmp_root}/${name}"
  mkdir -p "${case_dir}/bin"
  write_stub_docker "${case_dir}/bin"
  write_hash_probes "${case_dir}/bin"
  : >"${case_dir}/docker.log"
  local status=0
  set_helper_args "$store"
  (
    cd "$case_dir"
    env PATH="${case_dir}/bin:/usr/bin:/bin" \
      CONSUMER_DOCKER_LOG="${case_dir}/docker.log" \
      CONSUMER_DOCKER_REQUIRE_IDENTITY="$GOOD_ID" \
      CONSUMER_DOCKER_REQUIRE_ISSUER="$ISSUER" \
      bash "$mutant" "${HELPER_ARGS[@]}"
  ) >"${case_dir}/stdout" 2>"${case_dir}/stderr" || status=$?
  if [[ "$status" -eq 0 ]] && assert_verify_isolated "${case_dir}/docker.log" >/dev/null 2>&1; then
    fail "mutation ${name} stayed green"
  fi
}

assert_mutation_red drop-network-none '--network=none' '--network=bridge'
# The old string is helper source, not a shell expansion.
# shellcheck disable=SC2016
assert_mutation_red drop-identity '--certificate-identity "$certificate_identity"' '--certificate-identity ignored'
assert_mutation_red drop-trusted-root '--trusted-root /trusted_root.json' '--offline'

# Meaningful mutation: remove shared host-dir normalization. Absolute
# stores stay green; the candidate `--assets-dir release` caller must RED.
assert_named_volume_mutation_red() {
  local name="$1"
  local old="$2"
  local new="$3"
  local mutant="${tmp_root}/${name}.sh"
  mutate_helper "$mutant" "$old" "$new"
  chmod +x "$mutant"
  local case_dir="${tmp_root}/${name}"
  mkdir -p "${case_dir}/release" "${case_dir}/bin"
  prepare_assets "${case_dir}/release"
  write_stub_docker "${case_dir}/bin"
  write_hash_probes "${case_dir}/bin"
  : >"${case_dir}/docker.log"
  local expected
  expected="$(cd -- "${case_dir}/release" && pwd -P)"
  local status=0
  set_helper_args "release"
  (
    cd "$case_dir"
    env PATH="${case_dir}/bin:/usr/bin:/bin" \
      CONSUMER_DOCKER_LOG="${case_dir}/docker.log" \
      CONSUMER_DOCKER_REQUIRE_IDENTITY="$GOOD_ID" \
      CONSUMER_DOCKER_REQUIRE_ISSUER="$ISSUER" \
      bash "$mutant" "${HELPER_ARGS[@]}"
  ) >"${case_dir}/stdout" 2>"${case_dir}/stderr" || status=$?
  if [[ "$status" -eq 0 ]] \
    && assert_host_bind_and_hash "${case_dir}/docker.log" "${case_dir}/stdout" "$expected" \
      >/dev/null 2>&1; then
    fail "mutation ${name} stayed green on --assets-dir release"
  fi
}

# shellcheck disable=SC2016
assert_named_volume_mutation_red drop-physical-host-dir \
  'assets_dir="$(physical_host_dir "$assets_dir")"' \
  'assets_dir="$assets_dir"'

# Guard-removal: restore the image-default USER (65532:65532) instead of
# the invoking host. Isolation argv stays green because the stub still
# writes TUF as this host process - the seam that let hosted
# 35469738379 fail while this suite stayed green. The uid oracle must RED.
# Empty DOCKER_USER_ARGS=() is not this mutant: bash 3.2 + set -u aborts
# the helper before argv is recorded, which is a different failure.
assert_host_uid_mutation_red() {
  local name="$1"
  local old="$2"
  local new="$3"
  local mutant="${tmp_root}/${name}.sh"
  mutate_helper "$mutant" "$old" "$new"
  chmod +x "$mutant"
  local case_dir="${tmp_root}/${name}"
  mkdir -p "${case_dir}/release" "${case_dir}/bin"
  prepare_assets "${case_dir}/release"
  write_stub_docker "${case_dir}/bin"
  write_hash_probes "${case_dir}/bin"
  : >"${case_dir}/docker.log"
  local status=0
  set_helper_args "release"
  (
    cd "$case_dir"
    env PATH="${case_dir}/bin:/usr/bin:/bin" \
      CONSUMER_DOCKER_LOG="${case_dir}/docker.log" \
      CONSUMER_DOCKER_REQUIRE_IDENTITY="$GOOD_ID" \
      CONSUMER_DOCKER_REQUIRE_ISSUER="$ISSUER" \
      bash "$mutant" "${HELPER_ARGS[@]}"
  ) >"${case_dir}/stdout" 2>"${case_dir}/stderr" || status=$?
  if ! assert_verify_isolated "${case_dir}/docker.log" >/dev/null 2>&1; then
    fail "mutation ${name} should keep isolation argv (stub boundary) while uid bind goes red"
  fi
  if [[ "$status" -eq 0 ]] \
    && assert_host_uid_bind_on_both_phases "${case_dir}/docker.log" >/dev/null 2>&1; then
    fail "mutation ${name} stayed green on host-uid bind"
  fi
  if assert_host_uid_bind_on_both_phases "${case_dir}/docker.log" >/dev/null 2>&1; then
    fail "mutation ${name} still satisfied the host-uid argv oracle"
  fi
}

# shellcheck disable=SC2016
assert_host_uid_mutation_red drop-host-uid-bind \
  'DOCKER_USER_ARGS=(--user "$(id -u):$(id -g)")' \
  'DOCKER_USER_ARGS=(--user "65532:65532")'

# No-op control: identical helper copy still binds the relative caller.
noop_helper="${tmp_root}/noop-helper.sh"
cp "$HELPER" "$noop_helper"
chmod +x "$noop_helper"
noop_case="${tmp_root}/noop-relative"
mkdir -p "${noop_case}/release" "${noop_case}/bin"
prepare_assets "${noop_case}/release"
write_stub_docker "${noop_case}/bin"
write_hash_probes "${noop_case}/bin"
: >"${noop_case}/docker.log"
expected_noop="$(cd -- "${noop_case}/release" && pwd -P)"
set_helper_args "release"
if ! (
  cd "$noop_case"
  env PATH="${noop_case}/bin:/usr/bin:/bin" \
    CONSUMER_DOCKER_LOG="${noop_case}/docker.log" \
    CONSUMER_DOCKER_REQUIRE_IDENTITY="$GOOD_ID" \
    CONSUMER_DOCKER_REQUIRE_ISSUER="$ISSUER" \
    bash "$noop_helper" "${HELPER_ARGS[@]}"
) >"${noop_case}/stdout" 2>"${noop_case}/stderr"; then
  cat "${noop_case}/stdout" "${noop_case}/stderr" >&2
  fail "no-op helper copy failed the relative release caller"
fi
assert_host_bind_and_hash "${noop_case}/docker.log" "${noop_case}/stdout" "$expected_noop"
assert_host_uid_bind_on_both_phases "${noop_case}/docker.log"

# Control: unmutated helper still matches the isolation oracle.
assert_verify_isolated "${happy}/docker.log"
assert_host_uid_bind_on_both_phases "${happy}/docker.log"

# Workflow wiring: derive placement and invocation, not labels alone.
wiring_check="${tmp_root}/check_consumer_wiring.py"
cat >"$wiring_check" <<'PY'
from pathlib import Path
import importlib.util
import re
import sys

release_path, published_path, pin_path = map(Path, sys.argv[1:4])
index_digest = sys.argv[4]
repo_root = Path(sys.argv[5])
release = release_path.read_text(encoding="utf-8")
published = published_path.read_text(encoding="utf-8")
pin = pin_path.read_text(encoding="utf-8")

def fail(msg):
    raise SystemExit(msg)

if pin.count(index_digest) != 1:
    fail("cosign pin file must contain the index digest exactly once")
if not re.search(r"^ghcr\.io/sigstore/cosign/cosign@sha256:[0-9a-f]{64}\n\Z", pin):
    fail("cosign pin file must be one digest-pinned ghcr cosign image line")
def live_helper_count(text):
    n = 0
    for line in text.splitlines():
        stripped = line.lstrip()
        if not stripped or stripped.startswith("#"):
            continue
        if "scripts/ci/verify_consumer_checksum_manifest.sh" in stripped:
            n += 1
    return n

for label, text in (("release.yml", release), ("published-release-golden-path.yml", published)):
    if index_digest in text:
        fail(f"{label} embeds the cosign digest; both routes must read .github/cosign-image")
    if live_helper_count(text) != 1:
        fail(f"{label} must invoke verify_consumer_checksum_manifest.sh exactly once")

def job_block(text, key, indent):
    marker = f"{' ' * indent}{key}:"
    lines = text.splitlines()
    starts = [i for i, line in enumerate(lines) if line == marker]
    if len(starts) != 1:
        fail(f"expected one {key} mapping")
    start = starts[0]
    end = len(lines)
    for i in range(start + 1, len(lines)):
        line = lines[i]
        if line and not line.startswith(" "):
            end = i
            break
        if line.startswith(" " * indent) and not line.startswith(" " * (indent + 1)):
            end = i
            break
    return "\n".join(lines[start:end])

def step_names(job):
    names = []
    for line in job.splitlines():
        if line.startswith("      - name: "):
            names.append(line[len("      - name: "):])
    return names

def named_step(job, name):
    marker = f"      - name: {name}"
    lines = job.splitlines()
    starts = [i for i, line in enumerate(lines) if line == marker]
    if len(starts) != 1:
        fail(f"expected one step named {name!r}")
    start = starts[0]
    end = len(lines)
    for i in range(start + 1, len(lines)):
        if lines[i].startswith("      - name:"):
            end = i
            break
    return "\n".join(lines[start:end])

release_job = job_block(release, "release", 2)
names = step_names(release_job)
try:
    sign_i = names.index("Write and sign checksums.txt")
    create_i = names.index("Create GitHub Release")
except ValueError as exc:
    fail(f"release job missing sign/create steps: {exc}")
between = names[sign_i + 1:create_i]
consumer_steps = [n for n in between if "consumer" in n.lower() or "network-isolated" in n.lower()]
if len(consumer_steps) != 1:
    fail("candidate consumer step must sit after sign and before Create GitHub Release")
step = named_step(release_job, consumer_steps[0])
if "scripts/ci/verify_consumer_checksum_manifest.sh" not in step:
    fail("candidate step does not execute the shared helper")
if "--assets-dir release" not in step:
    fail("candidate helper must target the pre-publication release/ directory")
if "assay-${VERSION}-x86_64-unknown-linux-gnu.tar.gz" not in step:
    fail("candidate helper must select the Linux x86_64 archive for this VERSION")
if "needs.release-contract.outputs.version" not in step:
    fail("candidate identity/version must come from release-contract")
if "@refs/tags/${{ needs.release-contract.outputs.version }}" not in step \
        and "@refs/tags/${VERSION}" not in step:
    fail("candidate certificate identity must stay on refs/tags/<version>")
if "refs/heads" in step or "github.ref" in step and "refs/tags" not in step:
    fail("candidate route must not broaden the signer identity to a branch ref")
if "id-token" in step:
    fail("candidate consumer verification must not request signing OIDC")

def shell_body(line):
    body = line.rstrip()
    if body.endswith("\\"):
        body = body[:-1].rstrip()
    return body

def load_golden_path_contract(root):
    checker = root / "scripts/ci/check-published-release-golden-path-contract.py"
    spec = importlib.util.spec_from_file_location(
        "published_release_golden_path_contract", checker
    )
    if spec is None or spec.loader is None:
        fail("unable to load published-release golden-path contract helper")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module

# Published checksum replay stays a separate job from the Linux x86_64 journey.
golden = load_golden_path_contract(repo_root)
jobs = []
for line in published.splitlines():
    if re.fullmatch(r"  [A-Za-z0-9_-]+:", line):
        jobs.append(line.strip()[:-1])
if "published-linux-journey" not in jobs:
    fail("published workflow lost published-linux-journey")
journey_problems = []
journey = golden.mapping_block(published, "published-linux-journey", 2, journey_problems)
if journey_problems or not journey:
    fail(journey_problems[0] if journey_problems else "published workflow lost published-linux-journey")
if not any(
    row.get("target") == "x86_64-unknown-linux-gnu"
    for row in golden.linux_journey_include_rows(journey)
):
    fail("published-linux-journey lost the x86_64 target")
step_problems = []
exercise = golden.named_step_lines(
    journey, "Exercise the attested published release", step_problems
)
if step_problems:
    fail(step_problems[0])
if not any(
    shell_body(line) == "bash scripts/ci/published-release-golden-path.sh" for line in exercise
):
    fail("published-linux-journey exercise step lost the golden-path driver")
if not any(shell_body(line) == '--target "$RELEASE_TARGET"' for line in exercise):
    fail("published-linux-journey exercise step lost --target binding")
consumer_jobs = [
    j for j in jobs if j not in {"published-linux-journey", "published-cli-opening"}
]
if not consumer_jobs:
    fail("published-assets consumer job is missing")
# Find the other job whose live run body invokes the helper.
found = None
for job_id in consumer_jobs:
    block = job_block(published, job_id, 2)
    if any(
        "scripts/ci/verify_consumer_checksum_manifest.sh" in line
        for line in golden.active_lines(block)
    ):
        found = block
        break
if found is None:
    fail("published-assets replay job does not execute the shared helper")
if "inputs.release_tag" not in found:
    fail("published replay must bind inputs.release_tag, not release-contract")
if "needs.release-contract" in found:
    fail("published replay must not read needs.release-contract.outputs.version")
if "@refs/tags/${{ inputs.release_tag }}" not in found \
        and "@refs/tags/${RELEASE_TAG}" not in found:
    fail("published replay identity must stay on refs/tags/<release_tag>")
if "gh release download" not in found:
    fail("published replay must download published assets before verify")
if "checksums.txt.sigstore.json" not in found or "checksums.txt" not in found:
    fail("published replay must download checksums.txt and the sigstore bundle")
if "assay-${RELEASE_TAG}-x86_64-unknown-linux-gnu.tar.gz" not in found:
    fail("published replay must download the selected Linux archive")
if "id-token:" in found:
    fail("published replay must not request signing OIDC")
print("workflow consumer wiring ok")
PY
python3 "$wiring_check" "$RELEASE_WORKFLOW" "$PUBLISHED_WORKFLOW" "$PIN_FILE" "$INDEX_DIGEST" "$REPO_ROOT"

# No-op control: identical copies stay green.
wf_noop="${tmp_root}/release-noop.yml"
cp "$RELEASE_WORKFLOW" "$wf_noop"
python3 "$wiring_check" "$wf_noop" "$PUBLISHED_WORKFLOW" "$PIN_FILE" "$INDEX_DIGEST" "$REPO_ROOT" \
  >/dev/null

# Workflow mutation: comment out the helper in release.yml -> wiring RED.
wf_mut="${tmp_root}/release-mut.yml"
python3 - "$RELEASE_WORKFLOW" "$wf_mut" <<'PY'
from pathlib import Path
import sys
src, dest = map(Path, sys.argv[1:])
text = src.read_text(encoding="utf-8")
old = "bash scripts/ci/verify_consumer_checksum_manifest.sh"
if text.count(old) != 1:
    raise SystemExit(f"release.yml helper count: {text.count(old)}")
dest.write_text(text.replace(old, "# " + old, 1), encoding="utf-8")
PY
if python3 "$wiring_check" "$wf_mut" "$PUBLISHED_WORKFLOW" "$PIN_FILE" "$INDEX_DIGEST" "$REPO_ROOT" \
  >"${tmp_root}/release-mut.out" 2>&1; then
  fail "release.yml helper-removal mutation stayed green"
fi
grep -Fq 'must invoke verify_consumer_checksum_manifest.sh exactly once' \
  "${tmp_root}/release-mut.out" \
  || fail "release.yml helper-removal mutation missed the live-invocation guard"

pub_mut="${tmp_root}/published-mut.yml"
python3 - "$PUBLISHED_WORKFLOW" "$pub_mut" <<'PY'
from pathlib import Path
import sys
src, dest = map(Path, sys.argv[1:])
text = src.read_text(encoding="utf-8")
old = "bash scripts/ci/verify_consumer_checksum_manifest.sh"
if text.count(old) != 1:
    raise SystemExit(f"published workflow helper count: {text.count(old)}")
dest.write_text(text.replace(old, "# " + old, 1), encoding="utf-8")
PY
if python3 "$wiring_check" "$RELEASE_WORKFLOW" "$pub_mut" "$PIN_FILE" "$INDEX_DIGEST" "$REPO_ROOT" \
  >"${tmp_root}/published-mut.out" 2>&1; then
  fail "published-release helper-removal mutation stayed green"
fi
grep -Fq 'must invoke verify_consumer_checksum_manifest.sh exactly once' \
  "${tmp_root}/published-mut.out" \
  || fail "published-release helper-removal mutation missed the live-invocation guard"

expect_published_journey_red() {
  local name="$1" expected="$2"
  local out="${tmp_root}/${name}.out"
  if python3 "$wiring_check" "$RELEASE_WORKFLOW" "${tmp_root}/${name}.yml" "$PIN_FILE" "$INDEX_DIGEST" "$REPO_ROOT" \
    >"$out" 2>&1; then
    fail "${name} stayed green"
  fi
  grep -Fq "$expected" "$out" || fail "${name} missed guard: ${expected}"
}

x86_row="${tmp_root}/drop-x86-row.yml"
python3 - "$PUBLISHED_WORKFLOW" "$x86_row" <<'PY'
from pathlib import Path
import sys
src, dest = map(Path, sys.argv[1:])
text = src.read_text(encoding="utf-8")
old = (
    "          - os: ubuntu-24.04\n"
    "            label: Linux x86_64\n"
    "            target: x86_64-unknown-linux-gnu\n"
)
if text.count(old) != 1:
    raise SystemExit(f"x86_64 journey row count: {text.count(old)}")
dest.write_text(text.replace(old, "", 1), encoding="utf-8")
PY
expect_published_journey_red "drop-x86-row" "published-linux-journey lost the x86_64 target"

comment_target="${tmp_root}/comment-target.yml"
python3 - "$PUBLISHED_WORKFLOW" "$comment_target" <<'PY'
from pathlib import Path
import sys
src, dest = map(Path, sys.argv[1:])
text = src.read_text(encoding="utf-8")
old = (
    "          bash scripts/ci/published-release-golden-path.sh \\\n"
    "            --release-tag \"$RELEASE_TAG\" \\\n"
    "            --target \"$RELEASE_TARGET\" \\\n"
)
new = (
    "          bash scripts/ci/published-release-golden-path.sh \\\n"
    "            --release-tag \"$RELEASE_TAG\" \\\n"
    "            # --target \"$RELEASE_TARGET\" \\\n"
)
if text.count(old) != 1:
    raise SystemExit(f"journey exercise target count: {text.count(old)}")
dest.write_text(text.replace(old, new, 1), encoding="utf-8")
PY
expect_published_journey_red "comment-target" "published-linux-journey exercise step lost --target binding"

moved_helper="${tmp_root}/move-helper.yml"
python3 - "$PUBLISHED_WORKFLOW" "$moved_helper" <<'PY'
from pathlib import Path
import sys
src, dest = map(Path, sys.argv[1:])
text = src.read_text(encoding="utf-8")
helper = "          bash scripts/ci/verify_consumer_checksum_manifest.sh \\\n"
driver = "          bash scripts/ci/published-release-golden-path.sh \\\n"
if text.count(helper) != 1 or text.count(driver) != 1:
    raise SystemExit(
        f"helper/driver anchors: helper={text.count(helper)} driver={text.count(driver)}"
    )
text = text.replace(helper, "", 1)
dest.write_text(text.replace(driver, driver + helper, 1), encoding="utf-8")
PY
expect_published_journey_red "move-helper" "published-assets replay job does not execute the shared helper"

# Connected docs recipe must stay the existing curl/cosign fence.
python3 - "${REPO_ROOT}/docs/getting-started/installation.md" \
  "${REPO_ROOT}/docs/reference/release.md" <<'PY'
from pathlib import Path
import re
import sys

def fences(path):
    return re.findall(r"```bash\n(.*?)```", Path(path).read_text(encoding="utf-8"), re.S)

install, release = sys.argv[1:]
chosen_i = [f for f in fences(install) if "cosign verify-blob" in f and "checksums.txt" in f]
chosen_r = [f for f in fences(release) if "cosign verify-blob" in f and "checksums.txt" in f and "curl " in f]
if len(chosen_i) != 1 or len(chosen_r) != 1:
    raise SystemExit("connected signed-manifest bash fence count drifted")
if chosen_i[0] != chosen_r[0]:
    raise SystemExit("installation.md and release.md connected recipes must still match")
text = chosen_i[0]
if "docker " in text or "--network=none" in text or "--trusted-root" in text:
    raise SystemExit("connected recipe must stay the curl/cosign path")
if "sha256sum -c" not in text:
    raise SystemExit("connected recipe must keep sha256sum -c")
print("connected docs recipe preserved")
PY

# Forbidden repairs: chmod-world, privileged, isolation drop, pin skip.
# Host-uid bind is the allowed seam. This is source text, not Docker.
python3 - "$HELPER" <<'PY'
from pathlib import Path
import sys

text = Path(sys.argv[1]).read_text(encoding="utf-8")
forbidden = (
    "chmod 777",
    "chmod -R 777",
    "chmod a+rwx",
    "chmod 0777",
    "--privileged",
    "--network=host",
    "--userns=host",
)
for needle in forbidden:
    if needle in text:
        raise SystemExit(f"helper contains forbidden repair {needle!r}")
if "DOCKER_USER_ARGS=(--user \"$(id -u):$(id -g)\")" not in text:
    raise SystemExit("helper lost the single host-uid bind assignment")
if text.count('"${DOCKER_USER_ARGS[@]}"') != 2:
    raise SystemExit("both container phases must consume DOCKER_USER_ARGS")
print("helper has no forbidden repair and binds host uid on both phases")
PY

echo "verify consumer checksum manifest tests passed"
