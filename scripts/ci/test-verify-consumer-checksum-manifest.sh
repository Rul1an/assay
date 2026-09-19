#!/usr/bin/env bash
# Behavioral contract for the network-isolated consumer checksum helper (Refs #3119).
# Stub Docker/cosign + recorded argv. Does not prove live cryptography.
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

# Positive control.
assets="${tmp_root}/happy-assets"
prepare_assets "$assets"
happy="${tmp_root}/happy"
mkdir -p "$happy"
set_helper_args "$assets"
if ! run_helper "$happy" "${HELPER_ARGS[@]}" \
  >"${happy}/stdout" 2>"${happy}/stderr"; then
  cat "${happy}/stdout" "${happy}/stderr" >&2
  fail "positive consumer helper failed"
fi
grep -Fq 'Verified OK' "${happy}/stdout" || fail "positive helper did not print Verified OK"
grep -Fq "${ARCHIVE}: OK" "${happy}/stdout" || fail "positive helper did not print archive OK"
grep -Fq HASH_STEP_REACHED "${happy}/stdout" || fail "positive helper did not reach the hash step"
assert_verify_isolated "${happy}/docker.log"
# Owned scratch must not leak after success.
if find "$tmp_root" -type d -name 'tmp.*' -o -path '*/tuf-cache' | grep -q .; then
  # Helper scratch is under /tmp via mktemp, not case_dir; assert helper did not leave TUF in assets.
  :
fi
[[ ! -e "${assets}/tuf-cache" ]] || fail "helper must not write TUF state into assets-dir"

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

# Control: unmutated helper still matches the isolation oracle.
assert_verify_isolated "${happy}/docker.log"

# Workflow wiring: derive placement and invocation, not labels alone.
wiring_check="${tmp_root}/check_consumer_wiring.py"
cat >"$wiring_check" <<'PY'
from pathlib import Path
import re
import sys

release_path, published_path, pin_path = map(Path, sys.argv[1:4])
index_digest = sys.argv[4]
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

# Published-assets replay must be a sibling of the pinned linux-x86_64 job.
if "  linux-x86_64:" not in published:
    fail("published workflow lost linux-x86_64")
jobs = []
for line in published.splitlines():
    if re.fullmatch(r"  [A-Za-z0-9_-]+:", line):
        jobs.append(line.strip()[:-1])
if "linux-x86_64" not in jobs:
    fail("published workflow must keep linux-x86_64")
consumer_jobs = [j for j in jobs if j != "linux-x86_64" and j != "published-cli-opening"]
if not consumer_jobs:
    fail("published-assets consumer job is missing")
# Find the job whose run body invokes the helper.
found = None
for job_id in consumer_jobs:
    block = job_block(published, job_id, 2)
    if "scripts/ci/verify_consumer_checksum_manifest.sh" in block:
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
python3 "$wiring_check" "$RELEASE_WORKFLOW" "$PUBLISHED_WORKFLOW" "$PIN_FILE" "$INDEX_DIGEST"

# No-op control: identical copies stay green.
wf_noop="${tmp_root}/release-noop.yml"
cp "$RELEASE_WORKFLOW" "$wf_noop"
python3 "$wiring_check" "$wf_noop" "$PUBLISHED_WORKFLOW" "$PIN_FILE" "$INDEX_DIGEST" \
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
if python3 "$wiring_check" "$wf_mut" "$PUBLISHED_WORKFLOW" "$PIN_FILE" "$INDEX_DIGEST" \
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
if python3 "$wiring_check" "$RELEASE_WORKFLOW" "$pub_mut" "$PIN_FILE" "$INDEX_DIGEST" \
  >"${tmp_root}/published-mut.out" 2>&1; then
  fail "published-release helper-removal mutation stayed green"
fi
grep -Fq 'must invoke verify_consumer_checksum_manifest.sh exactly once' \
  "${tmp_root}/published-mut.out" \
  || fail "published-release helper-removal mutation missed the live-invocation guard"

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

echo "verify consumer checksum manifest tests passed"
