"""Bounded disposable working-tree copy for process/batch mutation.

process/batch measure in a unique temp copy of repo_root. The declared
user checkout is not written. Each materialize creates a new root with
tempfile.mkdtemp in the system temp directory; the root never lies under
repo_root.

cleanup removes only self.root, and only when lstat shows a directory
that is a direct child of system temp and carries the repo-keyed
muttree prefix. There is no stable pointer and no cross-run stale
delete. SIGKILL orphans stay inert until the OS reclaims temp.
The copy is not an atomic filesystem snapshot: concurrent external
writes can produce mixed bytes. Cleanup is best-effort; a normal
cleanup error or SIGKILL can leave an inert temp-root.

Never a sandbox. Never a git worktree. Never the #4 output ceiling.
Never #11 module isolation. Never #2 HEAD-vs-dirty provenance.

`.git` is omitted. Build rules that require git metadata in the tree are
unsupported. Symlinks, FIFOs, sockets, and devices are refused
fail-closed. Materialization trips file and byte ceilings during the
walk. Executable mode bits are preserved; timestamps and ownership are
not semantic input.
"""

from __future__ import annotations

import errno
import hashlib
import os
import shutil
import stat
import tempfile
from pathlib import Path

MATERIALIZATION_CAP_BYTES = 64 * 1024 * 1024
MATERIALIZATION_CAP_FILES = 10_000
COPY_CHUNK_BYTES = 64 * 1024
_TREE_PREFIX = "corpus-adequacy-muttree-"


class IsolationError(Exception):
    """The disposable working-tree copy could not be materialized."""


def _tree_key(repo_root: Path) -> str:
    return hashlib.sha256(str(Path(repo_root).resolve()).encode("utf-8")).hexdigest()[:16]


def _tree_prefix(repo_root: Path) -> str:
    return "%s%s" % (_TREE_PREFIX, _tree_key(repo_root))


def _contained(path: Path, root: Path) -> bool:
    try:
        path.resolve().relative_to(root.resolve())
        return True
    except ValueError:
        return False


def _owned_self_root(path: Path, repo_root: Path) -> bool:
    """lstat / direct system-temp child / repo-keyed prefix. Not ownership."""
    try:
        st = os.lstat(path)
    except OSError:
        return False
    if stat.S_ISLNK(st.st_mode) or not stat.S_ISDIR(st.st_mode):
        return False
    try:
        parent = path.parent.resolve()
        tmp = Path(tempfile.gettempdir()).resolve()
    except OSError:
        return False
    if parent != tmp:
        return False
    if not path.name.startswith(_tree_prefix(repo_root)):
        return False
    if _contained(path, Path(repo_root)):
        return False
    return True


def write_all(fd: int, buf: bytes) -> int:
    """Write every byte. os.write may return short.

    Partial progress advances the offset. Zero progress is refusal, not a
    spin. EINTR retries the same remaining slice.
    """
    sent = 0
    while sent < len(buf):
        try:
            n = os.write(fd, buf[sent:])
        except InterruptedError:
            continue
        except OSError as exc:
            if exc.errno == errno.EINTR:
                continue
            raise
        if n <= 0:
            raise IsolationError(
                "short write of %s bytes stopped at %s" % (len(buf), sent))
        sent += n
    return sent


def _copy_regular_bounded(
    src: Path,
    dest: Path,
    already: int,
    cap: int,
    *,
    chunk: int | None = None,
) -> int:
    """Copy one regular file in chunks. Memory is one small read, not the file."""
    if chunk is None:
        chunk = COPY_CHUNK_BYTES
    nofollow = getattr(os, "O_NOFOLLOW", None)
    if nofollow is None:
        raise IsolationError("no O_NOFOLLOW; refusing to copy %s" % src)
    fd = os.open(src, os.O_RDONLY | nofollow)
    written = 0
    out_fd = None
    try:
        st = os.fstat(fd)
        if not stat.S_ISREG(st.st_mode):
            raise IsolationError("not a regular file after open: %s" % src)
        dest.parent.mkdir(parents=True, exist_ok=True)
        out_fd = os.open(dest, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
        while True:
            used = already + written
            if used > cap:
                raise IsolationError("materialization exceeds the ceiling of %d bytes" % cap)
            room = cap - used
            buf = os.read(fd, min(chunk, room + 1))
            if not buf:
                break
            if used + len(buf) > cap:
                raise IsolationError("materialization exceeds the ceiling of %d bytes" % cap)
            written += write_all(out_fd, buf)
        os.fchmod(out_fd, stat.S_IMODE(st.st_mode))
        return written
    except Exception:
        if dest.exists():
            try:
                dest.unlink()
            except OSError:
                pass
        raise
    finally:
        if out_fd is not None:
            os.close(out_fd)
        os.close(fd)


class IsolatedMutationTree:
    """One owner for a unique-per-run working-tree copy and its cleanup."""

    def __init__(self, repo_root: Path | None = None) -> None:
        self.original_root = Path(repo_root) if repo_root is not None else None
        self.root: Path | None = None

    def materialize(
        self,
        repo_root: Path | None = None,
        *,
        cap: int | None = None,
        file_cap: int | None = None,
    ) -> Path:
        if repo_root is not None:
            self.original_root = Path(repo_root)
        if self.original_root is None:
            raise IsolationError("original root is missing")
        if cap is None:
            cap = MATERIALIZATION_CAP_BYTES
        if file_cap is None:
            file_cap = MATERIALIZATION_CAP_FILES
        src_root = self.original_root
        try:
            src_resolved = src_root.resolve()
        except OSError as exc:
            raise IsolationError("original root is missing: %s" % src_root) from exc
        if not src_resolved.is_dir():
            raise IsolationError("original root is missing: %s" % src_root)

        sys_tmp = Path(tempfile.gettempdir()).resolve()
        if _contained(sys_tmp, src_resolved) or sys_tmp == src_resolved:
            raise IsolationError(
                "system temp dir %s lies under repo_root %s, so a copy "
                "would nest inside itself" % (sys_tmp, src_resolved))

        unique = Path(tempfile.mkdtemp(prefix=_tree_prefix(src_resolved), dir=str(sys_tmp)))
        if _contained(unique, src_resolved):
            shutil.rmtree(unique, ignore_errors=True)
            raise IsolationError("temp-root %s lies under repo_root %s" % (unique, src_resolved))
        self.root = unique
        try:
            self._copy_dir(src_resolved, unique, src_resolved, 0, 0, cap, file_cap)
        except IsolationError:
            self.cleanup()
            raise
        except OSError as exc:
            self.cleanup()
            raise IsolationError("could not materialize isolated tree: %s" % exc) from exc
        return unique

    def _copy_dir(
        self,
        src_dir: Path,
        dest_dir: Path,
        repo_root: Path,
        copied_bytes: int,
        copied_files: int,
        cap: int,
        file_cap: int,
    ) -> tuple[int, int]:
        dest_dir.mkdir(parents=True, exist_ok=True)
        try:
            entries = os.scandir(src_dir)
        except FileNotFoundError as exc:
            raise IsolationError("missing path during materialize: %s" % src_dir) from exc
        with entries:
            for entry in entries:
                name = entry.name
                if name == ".git":
                    continue
                try:
                    st = entry.stat(follow_symlinks=False)
                except OSError as exc:
                    raise IsolationError("could not lstat %s: %s" % (entry.path, exc)) from exc
                if stat.S_ISLNK(st.st_mode):
                    raise IsolationError("symlink refused: %s" % entry.path)
                if copied_files + 1 > file_cap:
                    raise IsolationError(
                        "materialization exceeds the entry ceiling of %d" % file_cap)
                if stat.S_ISDIR(st.st_mode):
                    copied_files += 1
                    copied_bytes, copied_files = self._copy_dir(
                        Path(entry.path), dest_dir / name, repo_root,
                        copied_bytes, copied_files, cap, file_cap)
                    continue
                if stat.S_ISREG(st.st_mode):
                    dest = dest_dir / name
                    n = _copy_regular_bounded(
                        Path(entry.path), dest, copied_bytes, cap)
                    copied_files += 1
                    copied_bytes += n
                    continue
                if stat.S_ISFIFO(st.st_mode):
                    kind = "fifo"
                elif stat.S_ISSOCK(st.st_mode):
                    kind = "socket"
                else:
                    kind = "device"
                raise IsolationError("refusing %s: %s" % (kind, entry.path))
        return copied_bytes, copied_files

    def cleanup(self) -> None:
        root = self.root
        repo = self.original_root
        self.root = None
        if root is None or repo is None:
            return
        if _owned_self_root(root, repo):
            shutil.rmtree(root, ignore_errors=True)

    def __enter__(self) -> Path:
        return self.materialize()

    def __exit__(self, *exc) -> None:
        self.cleanup()
