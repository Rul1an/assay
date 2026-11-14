"""Logical evidence-byte budget and dispatch-count reservation for the C2 baseline-only carrier.

Pattern adapted, with attribution, from refund-c3-mutation-2026-10/budget.py (not imported from there).

Domain (RESOURCE-REVIEW.md): logical file-content bytes, including `.pending-*` transients, summed over every
counted store (the run root with its CA export, subject copy, prefix and close roots and operator files, and a
later retained copy). Not RAM, not filesystem block allocation, not a host quota. Excluded: CA's isolated subject
copy and lock files in the system temp directory, interpreter memory and filesystem metadata.

The 5 GiB host-free floor is a separate check. It runs at every reservation and before every operator write,
and samples each filesystem involved: the temp paths, every store (a store that does not exist yet, such as a
fresh retention destination, is probed at its nearest existing ancestor) and, for an operator write, the target
directory. Each sample records its role (temp, store or target), the path and the probed path.

Policy: before every public CA operation and at every receipt hook, require
    used + INTERVAL_RESERVE + copy_allowance <= LIMIT.
INTERVAL_RESERVE (128 MiB) exceeds every source-derived maximum write between two consecutive check points of
the pinned baseline-only route (SOURCE_MAXIMA_MIB); the close allowance E is measured from the prefix root.
The 20 baseline dispatches are reserved before observe_prefix starts, so a 21st is refused before it begins.
"""
import os
import shutil
import stat
from pathlib import Path

MIB = 1 << 20
LIMIT = 256 * MIB
INTERVAL_RESERVE = 128 * MIB
MIN_FREE = 5 * 1024 * MIB
MAX_CALLS = 20
# RESOURCE-REVIEW.md, "Interval maxima" (MiB, excluding the measured close allowance E).
SOURCE_MAXIMA_MIB = {"prefix-entry": 84, "prefix-interhook": 76, "prefix-terminal": 48, "close-fixed": 16}


def _existing_anchor(path):
    """The nearest existing ancestor (lexically, no symlink resolution): where a fresh path's bytes would land."""
    path = Path(path)
    while not os.path.lexists(path) and path.parent != path:
        path = path.parent
    return path


class BudgetRefused(Exception):
    """A required reservation does not fit; nothing further may be written or dispatched."""


def root_bytes(root):
    """Conservative copy allowance: every regular file's logical size under root."""
    return _walk_bytes(Path(root))


def _walk_bytes(root):
    if not root.exists():
        return 0
    total = 0
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        for name in dirnames + filenames:
            full = os.path.join(dirpath, name)
            mode = os.lstat(full).st_mode
            if stat.S_ISLNK(mode):
                raise BudgetRefused("symbolic link inside a counted store: " + full)
            if stat.S_ISREG(mode):
                total += os.lstat(full).st_size
    return total


class Budget:
    def __init__(self, stores, *, limit=LIMIT, reserve=INTERVAL_RESERVE, min_free=MIN_FREE,
                 max_calls=MAX_CALLS, free_paths=(), disk_usage=shutil.disk_usage):
        self.stores = [Path(s) for s in stores]
        self.limit, self.reserve, self.min_free, self.max_calls = limit, reserve, min_free, max_calls
        self.free_paths = [Path(p) for p in free_paths]
        self.disk_usage = disk_usage
        self.calls_reserved = 0
        self.verified_receipts = 0  # counts hook calls (verified receipts), not dispatch attempts
        self.op, self.op_calls, self.op_limit = None, 0, 0
        self.decisions = []
        self.free_samples = []

    def add_store(self, path):
        self.stores.append(Path(path))

    def used(self):
        return sum(_walk_bytes(s) for s in self.stores)

    def _check_free(self, label, targets=()):
        """Sample every involved filesystem; refuse if any is below the floor. Nothing is written here."""
        roles = [("temp", p) for p in self.free_paths] + [("store", s) for s in self.stores]
        roles += [("target", Path(t)) for t in targets]
        for role, path in roles:
            probed = _existing_anchor(path)
            free = self.disk_usage(probed).free
            ok = free >= self.min_free
            self.free_samples.append({"label": label, "role": role, "path": str(path), "probed": str(probed),
                                      "free": free, "ok": ok})
            if not ok:
                raise BudgetRefused(f"host free space {free} below {self.min_free} at {probed} ({role} {path})")

    def require(self, label, copy_allowance=0, reserve=None):
        reserve = self.reserve if reserve is None else reserve
        used = self.used()
        need = used + reserve + copy_allowance
        entry = {"label": label, "used": used, "reserve": reserve,
                 "copy_allowance": copy_allowance, "limit": self.limit, "ok": need <= self.limit}
        self.decisions.append(entry)
        if not entry["ok"]:
            raise BudgetRefused(f"{label}: used {used} + reserve {reserve} + copy {copy_allowance} > {self.limit}")
        self._check_free(label)

    def require_bytes(self, label, n):
        """Exact non-CA reservation (retention copy): used + n <= limit, then the host-free check."""
        used = self.used()
        entry = {"label": label, "used": used, "bytes": n, "limit": self.limit, "ok": used + n <= self.limit}
        self.decisions.append(entry)
        if not entry["ok"]:
            raise BudgetRefused(f"{label}: used {used} + {n} > {self.limit}")
        self._check_free(label)

    def begin(self, op, planned_calls, copy_allowance=0):
        """Reserve the operation's frozen dispatch count and its entry interval before it starts."""
        if self.calls_reserved + planned_calls > self.max_calls:
            self.decisions.append({"label": op + "-calls", "reserved": self.calls_reserved,
                                   "planned": planned_calls, "max": self.max_calls, "ok": False})
            raise BudgetRefused(f"{op}: {self.calls_reserved} + {planned_calls} dispatches exceed {self.max_calls}")
        self.require(op + "-entry", copy_allowance)
        self.calls_reserved += planned_calls
        self.op, self.op_calls, self.op_limit = op, 0, planned_calls

    def hook(self, receipt, path):
        """CA on_verified_receipt: runs after a receipt is durable, before the next dispatch."""
        self.verified_receipts += 1
        self.op_calls += 1
        if self.op_calls > self.op_limit:
            raise BudgetRefused(f"{self.op}: dispatch {self.op_calls} beyond reserved {self.op_limit}")
        tail = "terminal" if self.op_calls == self.op_limit else "interhook"
        self.require(f"{self.op}-{tail}")

    def end(self):
        self.op, self.op_calls, self.op_limit = None, 0, 0

    def _inside_store(self, path):
        path = Path(path).resolve()
        return any(path == s.resolve() or s.resolve() in path.parents for s in self.stores)

    def operator_write(self, path, data):
        """Exact-size pre-write check, then an exclusive, fsynced write."""
        path = Path(path)
        if not self._inside_store(path.parent):
            raise BudgetRefused("operator write outside counted stores: " + str(path))
        used = self.used()
        ok = used + len(data) <= self.limit
        self.decisions.append({"label": "operator-write", "path": path.name, "bytes": len(data),
                               "used": used, "limit": self.limit, "ok": ok})
        if not ok:
            raise BudgetRefused(f"operator write {path.name}: {used} + {len(data)} > {self.limit}")
        self._check_free("operator-write", targets=[path.parent])
        fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
        with os.fdopen(fd, "wb") as stream:
            stream.write(data)
            stream.flush()
            os.fsync(stream.fileno())

    def record(self):
        return {"limit": self.limit, "reserve": self.reserve, "min_free": self.min_free,
                "max_calls": self.max_calls, "calls_reserved": self.calls_reserved,
                "verified_receipts": self.verified_receipts, "decisions": list(self.decisions),
                "free_samples": list(self.free_samples)}
