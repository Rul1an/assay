# shellcheck shell=bash

without_git_context() {
  env \
    -u GIT_ALTERNATE_OBJECT_DIRECTORIES \
    -u GIT_COMMON_DIR \
    -u GIT_DIR \
    -u GIT_INDEX_FILE \
    -u GIT_OBJECT_DIRECTORY \
    -u GIT_WORK_TREE \
    "$@"
}

hermetic_git() {
  local root="$1"
  shift
  without_git_context env \
    GIT_CONFIG_NOSYSTEM=1 \
    GIT_CONFIG_GLOBAL=/dev/null \
    GIT_CEILING_DIRECTORIES="$(dirname "$root")" \
    git -C "$root" \
      -c core.excludesFile= \
      -c core.attributesFile= \
      "$@"
}

snapshot_tree() {
  local root="$1"
  local snapshot_state snapshot_status
  snapshot_state="$(mktemp -d)"
  if ! hermetic_git "$root" ls-files -z --cached --others --exclude-standard \
      > "$snapshot_state/worktree-paths" \
    || ! hermetic_git "$root" ls-files -z --stage \
      > "$snapshot_state/index-stages" \
    || ! hermetic_git "$root" --no-optional-locks \
      status --porcelain=v1 -z --untracked-files=all \
      > "$snapshot_state/porcelain"
  then
    echo "snapshot could not collect hermetic Git state" >&2
    rm -rf "$snapshot_state"
    return 1
  fi

  if SNAPSHOT_ROOT="$root" python3 /dev/fd/3 \
      "$snapshot_state/worktree-paths" \
      "$snapshot_state/index-stages" \
      "$snapshot_state/porcelain" 3<<'PY'
import hashlib
import json
import os
from pathlib import Path
import stat
import sys

MAX_LIST_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 200_000
MAX_TOTAL_CONTENT_BYTES = 2 * 1024 * 1024 * 1024

if len(sys.argv) != 4:
    raise SystemExit("snapshot helper requires worktree, index, and porcelain inputs")


def read_bounded(path: str, label: str) -> bytes:
    source = Path(path)
    if source.stat().st_size > MAX_LIST_BYTES:
        raise SystemExit(f"snapshot {label} exceeds 64 MiB")
    with source.open("rb") as handle:
        payload = handle.read(MAX_LIST_BYTES + 1)
    if len(payload) > MAX_LIST_BYTES:
        raise SystemExit(f"snapshot {label} exceeds 64 MiB")
    return payload


def nul_records(payload: bytes, label: str) -> list[bytes]:
    records = payload.rstrip(b"\0").split(b"\0") if payload else []
    if len(records) > MAX_ENTRIES:
        raise SystemExit(f"snapshot {label} exceeds 200000 entries")
    return records


worktree_paths = nul_records(read_bounded(sys.argv[1], "path list"), "path list")
index_records = nul_records(read_bounded(sys.argv[2], "index stages"), "index stages")
porcelain_records = nul_records(read_bounded(sys.argv[3], "porcelain status"), "porcelain status")

for record in sorted(index_records):
    display_record = json.dumps(os.fsdecode(record), ensure_ascii=True)
    print(f"index-stage\t{display_record}")

# Keep porcelain records positional: `-z` rename records are paired by order.
for position, record in enumerate(porcelain_records):
    display_record = json.dumps(os.fsdecode(record), ensure_ascii=True)
    print(f"porcelain\t{position}\t{display_record}")

root = os.fsencode(os.environ["SNAPSHOT_ROOT"])
total_content = 0
for path in sorted(worktree_paths):
    absolute = os.path.join(root, path)
    if not os.path.lexists(absolute):
        digest = hashlib.sha256()
        digest.update(len(path).to_bytes(8, "big"))
        digest.update(path)
        digest.update(b"missing")
        display_path = json.dumps(os.fsdecode(path), ensure_ascii=True)
        print(f"worktree\t{digest.hexdigest()}\t{display_path}")
        continue
    metadata = os.lstat(absolute)
    file_type = stat.S_IFMT(metadata.st_mode)
    digest = hashlib.sha256()
    digest.update(len(path).to_bytes(8, "big"))
    digest.update(path)
    digest.update(file_type.to_bytes(4, "big"))

    if stat.S_ISREG(metadata.st_mode):
        total_content += metadata.st_size
        if total_content > MAX_TOTAL_CONTENT_BYTES:
            raise SystemExit("snapshot regular-file content exceeds 2 GiB")
        digest.update(metadata.st_size.to_bytes(8, "big"))
        with open(absolute, "rb") as handle:
            while chunk := handle.read(1024 * 1024):
                digest.update(chunk)
    elif stat.S_ISLNK(metadata.st_mode):
        target = os.readlink(absolute)
        target_bytes = os.fsencode(target)
        digest.update(len(target_bytes).to_bytes(8, "big"))
        digest.update(target_bytes)

    display_path = json.dumps(os.fsdecode(path), ensure_ascii=True)
    print(f"worktree\t{digest.hexdigest()}\t{display_path}")
PY
  then
    snapshot_status=0
  else
    snapshot_status=$?
  fi
  rm -rf "$snapshot_state"
  return "$snapshot_status"
}
