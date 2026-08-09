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
  hermetic_git "$root" ls-files -z --cached --others --exclude-standard \
    | SNAPSHOT_ROOT="$root" python3 /dev/fd/3 3<<'PY'
import hashlib
import json
import os
import stat
import sys

MAX_LIST_BYTES = 64 * 1024 * 1024
MAX_ENTRIES = 200_000
MAX_TOTAL_CONTENT_BYTES = 2 * 1024 * 1024 * 1024

listing = sys.stdin.buffer.read(MAX_LIST_BYTES + 1)
if len(listing) > MAX_LIST_BYTES:
    raise SystemExit("snapshot path list exceeds 64 MiB")

paths = listing.rstrip(b"\0").split(b"\0") if listing else []
if len(paths) > MAX_ENTRIES:
    raise SystemExit("snapshot path list exceeds 200000 entries")

root = os.fsencode(os.environ["SNAPSHOT_ROOT"])
total_content = 0
for path in sorted(paths):
    absolute = os.path.join(root, path)
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
    print(f"{digest.hexdigest()}\t{display_path}")
PY
}
