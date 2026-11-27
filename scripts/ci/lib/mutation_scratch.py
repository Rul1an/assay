#!/usr/bin/env python3
"""A fresh scratch copy of a worktree for mutation tests.

A mutation test writes mutants. It must never write the tree it tests: on SIGKILL or power loss no
handler runs, and a restore that never ran leaves the mutant in the source. So the mutants live in a
copy, made fresh for each run:

- the copy holds the worktree's current bytes, tracked and untracked but not ignored; not HEAD and
  not the index, so uncommitted work is what gets tested;
- every file is created new, with a fresh mtime and the source's mode; a symlink or other
  non-regular path is refused rather than followed, and nothing is hardlinked;
- the copy gets its own Git repository, so a tool that asks Git for the toplevel finds the copy;
- Git runs without the inherited GIT_* environment, which under a hook points at the source.

Mutant builds get `<target>/mutation-scratch`, never the ordinary artifact directory. Cargo hashes
path packages relative to the workspace root and judges freshness by mtime, so a copy that shared
the real tree's artifacts would leave a mutant the real tree then reports as Fresh.

Usage: mutation_scratch.py ROOT    prints the scratch directory, then the build directory.
"""
from __future__ import annotations

import os
import shutil
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

BUILD_SUBDIR = "mutation-scratch"


def git_env() -> dict[str, str]:
    """The environment for every Git and cargo call a mutation run makes: no inherited GIT_*."""
    return {key: value for key, value in os.environ.items() if not key.startswith("GIT_")}


def refuse(message: str) -> None:
    raise SystemExit(f"mutation scratch: {message}")


def target_base(root: Path) -> Path:
    configured = os.environ.get("CARGO_TARGET_DIR")
    base = Path(configured) if configured else Path("target")
    return Path(os.path.abspath(base if base.is_absolute() else root / base))


def worktree_files(root: Path) -> list[str]:
    listing = subprocess.run(
        ["git", "-C", str(root), "ls-files", "-z", "--cached", "--others", "--exclude-standard"],
        env=git_env(), check=True, capture_output=True).stdout
    return sorted({os.fsdecode(entry) for entry in listing.split(b"\0") if entry})


def copy_worktree(root: Path, dest: Path) -> None:
    base = str(target_base(root))
    for rel in worktree_files(root):
        source = os.path.join(root, rel)
        try:
            info = os.lstat(source)
        except FileNotFoundError:
            continue  # deleted in the worktree, so absent from the copy as well
        if not stat.S_ISREG(info.st_mode) or os.path.realpath(source) != source:
            refuse(f"{rel} is not a regular file reached without a link")
        if os.path.commonpath([base, source]) == base:
            refuse(f"{rel} lies in the cargo target directory {base}, which must not be copied")
        copy = dest / rel
        copy.parent.mkdir(parents=True, exist_ok=True)
        with open(source, "rb") as reader, open(copy, "xb") as writer:  # "x": always a new file
            shutil.copyfileobj(reader, writer)
        os.chmod(copy, stat.S_IMODE(info.st_mode))


def assert_separate(source: Path, copy: Path) -> None:
    """The file a mutation is about to write must not be the source under another name."""
    a, b = os.stat(source), os.stat(copy)
    if (a.st_dev, a.st_ino) == (b.st_dev, b.st_ino):
        refuse(f"{copy} is the same file as {source}")


def prepare(root: Path) -> tuple[Path, Path]:
    """Copy `root` into a new scratch directory; return it and the build directory for mutants."""
    root = Path(os.path.realpath(root))
    # TMPDIR is read now, not from tempfile's cache, so a caller's scratch location always holds.
    parent = os.environ.get("TMPDIR") or None
    scratch = Path(os.path.realpath(tempfile.mkdtemp(prefix="assay-mutation-scratch.", dir=parent)))
    try:
        copy_worktree(root, scratch)
        subprocess.run(["git", "init", "-q", str(scratch)], env=git_env(), check=True, capture_output=True)
        top = subprocess.run(["git", "-C", str(scratch), "rev-parse", "--show-toplevel"],
                             env=git_env(), check=True, capture_output=True, text=True).stdout.strip()
        if os.path.realpath(top) != str(scratch):
            refuse(f"Git resolves the scratch copy to {top}")
    except BaseException:
        shutil.rmtree(scratch, ignore_errors=True)
        raise
    return scratch, target_base(root) / BUILD_SUBDIR


def main(argv: list[str]) -> int:
    if len(argv) != 2:
        print("usage: mutation_scratch.py ROOT", file=sys.stderr)
        return 2
    scratch, build = prepare(Path(argv[1]))
    # One path per line is the whole protocol, so a path holding a line break would hand the caller
    # the part before it. Remove what this run allocated and refuse instead.
    if any(char in str(path) for path in (scratch, build) for char in "\r\n"):
        shutil.rmtree(scratch, ignore_errors=True)
        print("mutation scratch: refusing a scratch or build path that contains a line break", file=sys.stderr)
        return 1
    print(scratch)
    print(build)
    return 0


if __name__ == "__main__":
    raise SystemExit(main(sys.argv))
