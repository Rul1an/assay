#!/usr/bin/env python3
"""Retain a finished C2 baseline run root into a fresh destination, with a verified byte inventory.

Pattern adapted, with attribution, from refund-c3-mutation-2026-10/retain.py (not imported from there). The
baseline-only route has no ledger, so there is one store, `run`. The original run root is only read, never moved
or replaced. Before anything is written, the exact number of bytes to copy plus the inventory size is reserved
against the logical evidence-byte budget over all counted stores (source and destination). Every source is
re-hashed when copied, every copy is re-read and checked, and inventory.json is written last: a destination
without it is an incomplete retention.

Byte-correct retention does not certify the attempt. The inventory also records `finalization`, computed from the
retained bytes: `complete` only when operator/result.json and operator/provenance.json both exist, the provenance
names the SHA256 of exactly that result.json, and no publication-failure evidence is present: neither
operator/result.pending.json (removed only by a successful publish) nor operator/persistence-failure.json (review
1b5 F5). The carrier writes result.pending.json, then the provenance, and only then publishes result.json, so
matching hashes alone (for example identical pre-existing bytes) never prove publication. Otherwise it is
`incomplete`. verify() recomputes it, and the
CLI exits 4 after retaining an incomplete pair, so a retention never reads as a successful attempt.
"""
import argparse
import hashlib
import json
import os
import stat
import sys
import tempfile
from pathlib import Path

import budget

SCHEMA = "refund.c2-baseline-retention.v0"
STORES = ("run",)


class RetentionRefused(Exception):
    """Retention cannot proceed or a retained tree does not verify."""


def sha(raw):
    return hashlib.sha256(raw).hexdigest()


def _canonical(value):
    return json.dumps(value, sort_keys=True, separators=(",", ":"), ensure_ascii=True).encode()


def _files(root):
    """Sorted (relative path, bytes) of every regular file; anything else refuses."""
    root = Path(root)
    if root.is_symlink() or not root.is_dir():
        raise RetentionRefused("source is not a plain directory: " + str(root))
    found = []
    for dirpath, dirnames, filenames in os.walk(root, followlinks=False):
        for name in dirnames + filenames:
            full = Path(dirpath) / name
            mode = os.lstat(full).st_mode
            if stat.S_ISLNK(mode):
                raise RetentionRefused("symbolic link in retention source: " + str(full))
            if stat.S_ISDIR(mode):
                continue
            if not stat.S_ISREG(mode):
                raise RetentionRefused("non-regular file in retention source: " + str(full))
            found.append((full.relative_to(root).as_posix(), full.read_bytes()))
    return sorted(found)


def finalization(read):
    """complete only for a published result.json bound by the provenance written before it was published."""
    result, provenance = read("operator/result.json"), read("operator/provenance.json")
    if result is None or provenance is None:
        missing = [n for n, raw in (("result.json", result), ("provenance.json", provenance)) if raw is None]
        return {"state": "incomplete", "reason": "missing " + ", ".join(missing)}
    try:
        bound = json.loads(provenance).get("result_sha256")
    except (ValueError, AttributeError):
        return {"state": "incomplete", "reason": "provenance unreadable"}
    if bound != sha(result):
        return {"state": "incomplete", "reason": "provenance does not bind result.json"}
    markers = [n for n in ("result.pending.json", "persistence-failure.json") if read("operator/" + n) is not None]
    if markers:
        return {"state": "incomplete", "reason": "publication-failure evidence present: " + ", ".join(markers)}
    return {"state": "complete", "reason": None}


def plan(run_root):
    """Inventory the run root; reserve_bytes is exactly what copy() will write."""
    sources = {"run": Path(run_root)}
    found = _files(run_root)
    files = [{"store": "run", "path": rel, "size": len(raw), "sha256": sha(raw)} for rel, raw in found]
    total = sum(f["size"] for f in files)
    by_rel = dict(found)
    inventory = {"schema": SCHEMA, "stores": list(STORES), "files": files, "total_file_bytes": total,
                 "finalization": finalization(by_rel.get)}
    inventory_bytes = _canonical(inventory) + b"\n"
    return {"sources": sources, "files": files, "inventory_bytes": inventory_bytes,
            "reserve_bytes": total + len(inventory_bytes), "finalization": inventory["finalization"]}


def _write_new(path, raw):
    path.parent.mkdir(parents=True, exist_ok=True)
    fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(fd, "wb") as stream:
        stream.write(raw)
        stream.flush()
        os.fsync(stream.fileno())
    os.chmod(path, 0o444)


def _inside(child, parent):
    child, parent = Path(child).resolve(), Path(parent).resolve()
    return child == parent or parent in child.parents


def copy(plan_, dest, ledger):
    """Reserve, copy with per-file source verification, verify copies, then write the inventory."""
    dest = Path(dest)
    if dest.is_symlink() or dest.exists():
        raise RetentionRefused("retention destination already exists: " + str(dest))
    for source in plan_["sources"].values():
        if _inside(dest, source) or _inside(source, dest):
            raise RetentionRefused("destination overlaps a retention source: " + str(source))
    if not any(_inside(dest, s) for s in ledger.stores):
        ledger.add_store(dest)
    ledger.require_bytes("retention-copy", plan_["reserve_bytes"])
    dest.mkdir(mode=0o700)
    for f in plan_["files"]:
        raw = (plan_["sources"][f["store"]] / f["path"]).read_bytes()
        if len(raw) != f["size"] or sha(raw) != f["sha256"]:
            raise RetentionRefused("source changed since inventory: " + f["store"] + "/" + f["path"])
        _write_new(dest / f["store"] / f["path"], raw)
    _verify_files(dest, plan_["files"])
    _write_new(dest / "inventory.json", plan_["inventory_bytes"])
    verify(dest)
    return json.loads(plan_["inventory_bytes"])


def _verify_files(dest, files):
    expected = {f"{f['store']}/{f['path']}": f for f in files}
    present = {rel for rel, _ in _files(dest) if rel != "inventory.json"}
    if present != set(expected):
        raise RetentionRefused("retained file set differs from inventory")
    for rel, f in expected.items():
        raw = (dest / rel).read_bytes()
        if len(raw) != f["size"] or sha(raw) != f["sha256"]:
            raise RetentionRefused("retained copy differs from inventory: " + rel)


def verify(dest):
    """Re-check a retained tree against its own inventory; no extra or missing files."""
    dest = Path(dest)
    try:
        inventory = json.loads((dest / "inventory.json").read_bytes())
    except (OSError, ValueError) as exc:
        raise RetentionRefused("inventory unreadable: " + str(exc)) from None
    if inventory.get("schema") != SCHEMA or inventory.get("stores") != list(STORES):
        raise RetentionRefused("unknown inventory schema")
    _verify_files(dest, inventory["files"])

    def read(rel):
        path = dest / "run" / rel
        return path.read_bytes() if path.is_file() else None
    if inventory.get("finalization") != finalization(read):
        raise RetentionRefused("recorded finalization differs from the retained bytes")
    return inventory


def main():
    parser = argparse.ArgumentParser(description=__doc__.split("\n", 1)[0])
    parser.add_argument("--run-root", type=Path)
    parser.add_argument("--dest", type=Path, required=True)
    parser.add_argument("--verify", action="store_true", help="only verify an existing retained tree")
    args = parser.parse_args()
    try:
        if args.verify:
            inventory = verify(args.dest)
        else:
            if args.run_root is None:
                parser.error("--run-root is required to retain")
            ledger = budget.Budget([args.run_root, args.dest], free_paths=[tempfile.gettempdir()])
            inventory = copy(plan(args.run_root), args.dest, ledger)
    except (RetentionRefused, budget.BudgetRefused) as exc:
        print("retention refused: " + str(exc), file=sys.stderr)
        return 2
    print(json.dumps({"files": len(inventory["files"]), "total_file_bytes": inventory["total_file_bytes"],
                      "finalization": inventory["finalization"]}))
    if inventory["finalization"]["state"] != "complete":
        print("retained, but finalization is incomplete: not a complete attempt", file=sys.stderr)
        return 4
    return 0


if __name__ == "__main__":
    sys.exit(main())
