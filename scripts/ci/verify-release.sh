#!/usr/bin/env bash
# Read-only post-publish and pre-tag release checks. Exit 0=clean, 1=finding,
# 2=infra (missing tools, API/network failure, timeout, or unreadable input).
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=./scripts/ci/release_asset_contract.sh
source "$SCRIPT_DIR/release_asset_contract.sh"

REPO="${REPO:-Rul1an/assay}"
GH="${GH:-$(command -v gh)}"
PYTHON="${PYTHON:-$(command -v python3)}"
CURL="${CURL:-$(command -v curl)}"
export GH REPO

VERSION="${VERSION:-5.4.0}"
TAG="${TAG:-v${VERSION}}"
PIN_SHA="${PIN_SHA:-}"
MODE="post-publish"
WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

finding() { printf 'FINDING: %s\n' "$*" >&2; exit 1; }
infra() { printf 'INFRA: %s\n' "$*" >&2; exit 2; }

valid_sha() { [[ "${1:-}" =~ ^[0-9a-f]{40}$ ]]; }

expected_absent_attestation() {
  case "$1" in
    "assay-v${VERSION}-release-proof-kit.tar.gz"|"assay-v${VERSION}-release-proof-kit.tar.gz.sha256"|\
    "assay-v${VERSION}-release-provenance.json"|"assay-v${VERSION}-release-provenance.json.sha256") return 0 ;;
    *) return 1 ;;
  esac
}

classify_attestation() {
  local name="$1" status="$2"
  if [[ "$status" == 404 ]] && expected_absent_attestation "$name"; then
    printf '%s\n' "expected-absent"
    return 0
  fi
  [[ "$status" == 200 ]] || return 1
  printf '%s\n' "present"
}

# Capture command stdout before it is materialized as JSON; stderr remains visible.
bounded_capture() {
  local output="$1"
  shift
  "$PYTHON" - "$output" "$@" <<'PY'
import math
import os
import pathlib
import selectors
import signal
import subprocess
import sys
import time

output, command = pathlib.Path(sys.argv[1]), sys.argv[2:]
limit = 1024 * 1024
raw_timeout = os.environ.get("ASSAY_RELEASE_GH_TIMEOUT_SECONDS", "60")
try:
    timeout = float(raw_timeout)
except ValueError:
    raise SystemExit(
        "ASSAY_RELEASE_GH_TIMEOUT_SECONDS must be a positive finite number"
    )
if not math.isfinite(timeout) or timeout <= 0:
    raise SystemExit(
        "ASSAY_RELEASE_GH_TIMEOUT_SECONDS must be a positive finite number"
    )

try:
    process = subprocess.Popen(
        command,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        start_new_session=True,
    )
except OSError as error:
    raise SystemExit(f"could not execute {command[0]}: {error}")


class CaptureFailure(Exception):
    pass


def stop_process_group():
    try:
        os.killpg(process.pid, signal.SIGTERM)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=0.2)
    except subprocess.TimeoutExpired:
        pass
    # The direct child may exit while a TERM-resistant descendant remains.
    try:
        os.killpg(process.pid, signal.SIGKILL)
    except ProcessLookupError:
        pass
    try:
        process.wait(timeout=0.2)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=0.2)


stdout = bytearray()
total = 0
deadline = time.monotonic() + timeout
selector = selectors.DefaultSelector()
streams = ((process.stdout, "stdout"), (process.stderr, "stderr"))
for pipe, name in streams:
    os.set_blocking(pipe.fileno(), False)
    selector.register(pipe, selectors.EVENT_READ, name)

try:
    while selector.get_map():
        remaining = deadline - time.monotonic()
        if remaining <= 0:
            raise CaptureFailure(f"{command[0]} exceeded {timeout:g}s deadline")
        events = selector.select(timeout=remaining)
        if not events:
            continue
        for key, _ in events:
            try:
                chunk = os.read(key.fd, min(64 * 1024, limit + 1 - total))
            except BlockingIOError:
                continue
            except OSError as error:
                raise CaptureFailure(f"could not read {command[0]} output: {error}")
            if not chunk:
                selector.unregister(key.fileobj)
                key.fileobj.close()
                continue
            next_total = total + len(chunk)
            if next_total > limit:
                raise CaptureFailure(
                    f"{command[0]} response exceeds {limit} combined stdout+stderr bytes"
                )
            total = next_total
            if key.data == "stdout":
                stdout.extend(chunk)
            else:
                sys.stderr.buffer.write(chunk)
                sys.stderr.buffer.flush()

    remaining = deadline - time.monotonic()
    if remaining <= 0:
        raise CaptureFailure(f"{command[0]} exceeded {timeout:g}s deadline")
    try:
        process.wait(timeout=remaining)
    except subprocess.TimeoutExpired:
        raise CaptureFailure(f"{command[0]} exceeded {timeout:g}s deadline")
except CaptureFailure as error:
    stop_process_group()
    raise SystemExit(str(error))
finally:
    selector.close()
    for pipe, _ in streams:
        if not pipe.closed:
            pipe.close()

if process.returncode:
    raise SystemExit(f"{command[0]} exited {process.returncode}")
output.write_bytes(stdout)
PY
}

require_tools() {
  for command in "$GH" "$CURL" "$PYTHON"; do
    [[ -n "$command" ]] && command -v "$command" >/dev/null 2>&1 \
      || infra "required command is unavailable: $command"
  done
}

hash_file() {
  "$PYTHON" - "$1" <<'PY'
import hashlib
import pathlib
import sys
print(hashlib.sha256(pathlib.Path(sys.argv[1]).read_bytes()).hexdigest())
PY
}

verify_checksum() {
  "$PYTHON" - "$1" "$2" "$3" <<'PY'
import hashlib
import pathlib
import re
import sys

asset, checksum = map(pathlib.Path, sys.argv[1:3])
name = sys.argv[3]
content = checksum.read_bytes()
match = re.fullmatch(rb"([0-9a-f]{64})  " + re.escape(name.encode()) + rb"\n", content)
if not match:
    raise SystemExit("checksum is not GNU HEX64 two-space FILENAME newline form")
actual = hashlib.sha256(asset.read_bytes()).hexdigest().encode()
if actual != match.group(1):
    raise SystemExit("checksum does not match downloaded sibling bytes")
PY
}

parse_release() {
  "$PYTHON" - "$1" "$TAG" <<'PY'
import json
import pathlib
import sys

release = json.loads(pathlib.Path(sys.argv[1]).read_bytes())
if release.get("tag_name") != sys.argv[2] or release.get("draft") or release.get("prerelease"):
    raise SystemExit("release tag is missing, draft, or prerelease")
assets = release.get("assets")
if not isinstance(assets, list):
    raise SystemExit("release assets are not an array")
for asset in assets:
    name, digest, url = asset.get("name"), asset.get("digest"), asset.get("browser_download_url")
    if not all(isinstance(value, str) for value in (name, digest, url)):
        raise SystemExit("release asset has unreadable name, digest, or URL")
    print(f"{name}\t{digest}\t{url}")
print(f"TARGET\t{release.get('target_commitish', '')}")
PY
}

verify_attestation_json() {
  "$PYTHON" - "$1" <<'PY'
import json
import pathlib
import sys

document = json.loads(pathlib.Path(sys.argv[1]).read_bytes())
items = document.get("attestations")
if not isinstance(items, list) or len(items) != 1:
    raise SystemExit("attestation response must contain exactly one attestation")
item = items[0]
bundle = item.get("bundle")
media_type = bundle.get("dsseEnvelope", {}).get("payloadType") if isinstance(bundle, dict) else None
if media_type != "application/vnd.in-toto+json":
    raise SystemExit("attestation is not application/vnd.in-toto+json")
PY
}

copy_tag_file() {
  local sha="$1" path="$2" output="$3" response="$WORK/content.json"
  bounded_capture "$response" "$GH" api "repos/$REPO/contents/$path?ref=$sha" \
    || infra "could not read $path at $sha"
  "$PYTHON" - "$response" "$output" <<'PY'
import base64
import json
import pathlib
import sys

value = json.loads(pathlib.Path(sys.argv[1]).read_bytes()).get("content")
if not isinstance(value, str):
    raise SystemExit("GitHub content response has no base64 content")
output = pathlib.Path(sys.argv[2])
output.parent.mkdir(parents=True, exist_ok=True)
output.write_bytes(base64.b64decode(value, validate=False))
PY
}

check_tag_tree() {
  local sha="$1" tree="$2"
  valid_sha "$sha" || finding "refused non-canonical or abbreviated tag commit SHA: $sha"
  mkdir -p "$tree"
  local path
  for path in Cargo.toml .github/assay-release-tag README.md docs/index.md \
    docs/getting-started/index.md docs/getting-started/installation.md \
    docs/getting-started/quickstart.md docs/getting-started/ci-integration.md \
    docs/reference/cli/index.md docs/AIcontext/user-flows.md docs/use-cases/ci-gate.md; do
    copy_tag_file "$sha" "$path" "$tree/$path" || infra "could not materialize tag tree file: $path"
  done
  "$PYTHON" - "$tree" "$VERSION" <<'PY'
import pathlib
import re
import sys

root, version = pathlib.Path(sys.argv[1]), sys.argv[2]
cargo = (root / "Cargo.toml").read_text(encoding="utf-8")
section = re.search(r"(?ms)^\[workspace\.package\]$(.*?)(?=^\[|\Z)", cargo)
if not section or not re.search(rf'^version\s*=\s*"{re.escape(version)}"\s*$', section.group(1), re.M):
    raise SystemExit("workspace.package.version does not equal release version")
if (root / ".github/assay-release-tag").read_bytes() != b"v5.3.0\n":
    raise SystemExit("assay-release-tag is not exactly the separate v5.3.0 install pin")
install_counts = {
    "README.md": 1, "docs/getting-started/index.md": 1,
    "docs/getting-started/installation.md": 2, "docs/getting-started/quickstart.md": 1,
    "docs/getting-started/ci-integration.md": 4, "docs/reference/cli/index.md": 1,
    "docs/AIcontext/user-flows.md": 1, "docs/use-cases/ci-gate.md": 1,
}
pin = "cargo install assay-cli --version 5.3.0 --locked"
for path, count in install_counts.items():
    text = (root / path).read_text(encoding="utf-8")
    commands = re.findall(r"cargo install assay-cli --version \S+ --locked", text)
    if len(commands) != count or commands.count(pin) != count:
        raise SystemExit(f"{path}: install pin drift")
link = "Current release: [`v5.3.0`](https://github.com/Rul1an/assay/releases/tag/v5.3.0)"
for path in ("README.md", "docs/index.md"):
    text = (root / path).read_text(encoding="utf-8")
    if text.count("Current release:") != 1 or text.count(link) != 1:
        raise SystemExit(f"{path}: current release link drift")
PY
}

pre_tag() {
  [[ "$TAG" == "v$VERSION" ]] || finding "TAG must be VERSION-derived"
  valid_sha "$PIN_SHA" || finding "PIN_SHA must be exactly 40 lowercase hex characters"
  command -v git >/dev/null 2>&1 || infra "git is unavailable for pre-tag mode"
  git cat-file -e "${PIN_SHA}^{commit}" 2>/dev/null || infra "PIN_SHA is not a readable commit"
  check_tag_tree "$PIN_SHA" "$WORK/pre-tag-tree" || finding "pre-tag source and install-pin surfaces disagree"
  printf 'CLEAN: pre-tag %s at %s\n' "$VERSION" "$PIN_SHA"
}

post_publish() {
  require_tools
  [[ "$TAG" == "v$VERSION" ]] || finding "TAG must be VERSION-derived"
  local release="$WORK/release.json"
  bounded_capture "$release" "$GH" api "repos/$REPO/releases/tags/$TAG" \
    || infra "could not fetch release JSON"
  parse_release "$release" >"$WORK/assets.tsv" || finding "release JSON is malformed"
  awk -F '\t' '$1 != "TARGET" { print $1 }' "$WORK/assets.tsv" | LC_ALL=C sort >"$WORK/actual"
  release_expected_assets "$VERSION" | LC_ALL=C sort >"$WORK/expected"
  cmp -s "$WORK/actual" "$WORK/expected" || finding "release asset names do not exactly match the expected set"

  mkdir -p "$WORK/assets"
  while IFS=$'\t' read -r name digest url; do
    [[ "$name" != TARGET ]] || continue
    [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]] || finding "asset API digest is not sha256: $name"
    [[ "$url" == "https://github.com/$REPO/releases/download/$TAG/$name" ]] \
      || finding "asset API URL is unexpected: $name"
    "$CURL" --fail --silent --show-error --location --max-time 120 --max-filesize 1073741824 \
      --output "$WORK/assets/$name" "$url" || infra "could not download asset: $name"
  done <"$WORK/assets.tsv"

  local name digest url actual
  while IFS=$'\t' read -r name digest url; do
    [[ "$name" != TARGET ]] || continue
    if [[ "$name" == *.sha256 ]]; then
      verify_checksum "$WORK/assets/${name%.sha256}" "$WORK/assets/$name" "${name%.sha256}" \
        || finding "bad companion checksum: $name"
    elif [[ "$name" == server.json ]]; then
      actual="sha256:$(hash_file "$WORK/assets/$name")"
      [[ "$actual" == "$digest" ]] || finding "server.json bytes differ from API digest"
    fi
    local response="$WORK/attestation-${name}.json" status
    status="$("$CURL" --silent --show-error --location --max-time 60 --output "$response" --write-out '%{http_code}' \
      -H 'Accept: application/vnd.github+json' \
      "https://api.github.com/repos/$REPO/attestations/$digest")" || infra "could not query attestation: $name"
    classify_attestation "$name" "$status" >/dev/null || finding "unexpected attestation status $status: $name"
    [[ "$status" == 404 ]] || verify_attestation_json "$response" || finding "invalid attestation: $name"
  done <"$WORK/assets.tsv"

  local crate response newest
  for crate in assay-adapter-api assay-canonical assay-cli assay-common assay-core assay-evidence \
    assay-mcp-server assay-metrics assay-monitor assay-policy assay-registry assay-runner-core \
    assay-runner-linux assay-runner-schema assay-sim; do
    response="$WORK/crate-$crate.json"
    "$CURL" --fail --silent --show-error --location --max-time 60 --max-filesize 1048576 \
      -A "assay-release-oracle/1.0 (+https://github.com/$REPO)" \
      --output "$response" "https://crates.io/api/v1/crates/$crate" \
      || infra "could not query crates.io: $crate"
    newest="$("$PYTHON" - "$response" <<'PY'
import json, pathlib, sys
value = json.loads(pathlib.Path(sys.argv[1]).read_bytes()).get("crate", {}).get("newest_version")
if not isinstance(value, str):
    raise SystemExit("missing newest_version")
print(value)
PY
)" || infra "unreadable crates.io response: $crate"
    [[ "$newest" == "$VERSION" ]] || finding "crates.io newest_version mismatch: $crate is $newest"
  done

  local ref="$WORK/tag-ref.json" object_type peeled target
  bounded_capture "$ref" "$GH" api "repos/$REPO/git/ref/tags/$TAG" || infra "could not fetch tag ref"
  read -r object_type peeled < <("$PYTHON" - "$ref" <<'PY'
import json, pathlib, sys
value = json.loads(pathlib.Path(sys.argv[1]).read_bytes()).get("object", {})
print(value.get("type", ""), value.get("sha", ""))
PY
)
  if [[ "$object_type" == tag ]]; then
    valid_sha "$peeled" || finding "annotated tag object SHA is not canonical"
    bounded_capture "$WORK/tag-object.json" "$GH" api "repos/$REPO/git/tags/$peeled" || infra "could not fetch annotated tag"
    read -r object_type peeled < <("$PYTHON" - "$WORK/tag-object.json" <<'PY'
import json, pathlib, sys
value = json.loads(pathlib.Path(sys.argv[1]).read_bytes()).get("object", {})
print(value.get("type", ""), value.get("sha", ""))
PY
)
  fi
  [[ "$object_type" == commit ]] || finding "release tag does not peel to a commit"
  valid_sha "$peeled" || finding "tag commit SHA is abbreviated or malformed"
  check_tag_tree "$peeled" "$WORK/tag-tree" || finding "tag tree violates version or install-pin contract"
  target="$(awk -F '\t' '$1 == "TARGET" { print $2 }' "$WORK/assets.tsv")"
  if [[ "$target" =~ ^[0-9A-Fa-f]+$ ]]; then
    valid_sha "$target" || finding "release target_commitish is an abbreviated or malformed SHA"
    [[ "$target" == "$peeled" ]] || finding "release target_commitish does not equal peeled tag commit"
  else
    printf 'NOTE: release target_commitish is a name, not a SHA: %s\n' "$target"
  fi
  printf 'CLEAN: published release %s\n' "$TAG"
}

usage() { printf 'usage: verify-release.sh [--pre-tag [PIN_SHA]|--self-test]\n' >&2; exit 2; }
case "${1:-}" in
  --unit-expected-assets) [[ $# -eq 2 ]] || usage; release_expected_assets "$2"; exit 0 ;;
  --unit-installability-matrix) [[ $# -eq 2 ]] || usage; release_installability_matrix "$2"; exit 0 ;;
  --unit-installability-markdown) [[ $# -eq 2 ]] || usage; release_installability_markdown "$2"; exit 0 ;;
  --unit-validate-sha) [[ $# -eq 2 ]] || usage; valid_sha "$2" || exit 1; exit 0 ;;
  --unit-verify-attestation-json) [[ $# -eq 2 ]] || usage; verify_attestation_json "$2"; exit $? ;;
  --unit-classify-attestation)
    [[ $# -eq 3 && "$2" =~ v([0-9]+\.[0-9]+\.[0-9]+) ]] || usage
    VERSION="${BASH_REMATCH[1]}"
    classify_attestation "$2" "$3"
    exit $?
    ;;
  --pre-tag) MODE="pre-tag"; if [[ $# -eq 2 ]]; then PIN_SHA="$2"; elif [[ $# -ne 1 ]]; then usage; fi ;;
  --self-test) VERSION=5.3.0; TAG=v5.3.0 ;;
  "") ;;
  *) usage ;;
esac

if [[ "$MODE" == pre-tag ]]; then pre_tag; else post_publish; fi
