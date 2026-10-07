#!/usr/bin/env python3
"""Load one implementation module and report its outcomes, from a child process.

    python3 module_child.py <request.json>

Standard library only. Imported by nothing at runtime: the parent starts this
file as a program, so a corpus that misbehaves misbehaves here.

WHY THE CORPUS DOES NOT RUN IN THE TOOL
---------------------------------------
The module runner used to exec corpus source, and then call a MUTATED
entrypoint, inside the measuring process. A mutant is arbitrary code by
construction, and every way that ends badly was observed on this runner: an
endless mutant hung the tool with no report, SystemExit chose the tool's exit
code, os._exit(0) ended it at exit 0 with no report at all, a printing mutant
put 6.3 MB on the tool's own stdout, and a spawned descendant outlived the run.

The third is the one that matters most. Exit 0 with no output is what a CI gate
reads as "the adequacy check passed", so a mutant that ends its own measurer
scored better than one the corpus actually catches.

THE WIRE
--------
The protocol is one JSON object, written once at the end, to a descriptor
duplicated from stdout before anything of the corpus runs. Fd 1 is then pointed
at stderr, so everything the candidate writes -- print(), sys.stdout, or a
write straight to the descriptor -- lands on the inherited stderr pipe.

Candidate output is never captured here. Capturing it would buy a clean
protocol channel by moving an unbounded flood inside the very boundary meant to
bound it: the parent's ceiling applies to what crosses the pipes, so anything
held in this process is output the ceiling cannot see. Redirected, it streams
out and is charged like any other child's output.

The duplicate is not hidden. A child that scans its own descriptors can find it
and write whatever payload it likes, so this arrangement bounds accidents and
refuses output that did not arrive intact. It does not authenticate the channel
against a corpus written to forge a verdict, and the parent says so too.

WHAT AN OUTCOME MAY BE
----------------------
Whatever the entrypoint returns is carried as typed JSON, and the parent
compares the decoded values. That is the transport, and it is stated rather
than implied: a distinction JSON does not carry does not survive the trip, so
an outcome of ("a",) and one of ["a"] compare equal here, as do {1: x} and
{"1": x}. A value JSON cannot carry at all is reported as unsupported, and the
parent then declines to score that rule instead of guessing at it.
"""

from __future__ import annotations

import importlib.util
import json
import os
import sys
from pathlib import Path

SCHEMA = "corpus-adequacy.module-child.v0"


def load_module(source: str, tag: str, work_dir: Path):
    path = work_dir / ("impl_%s.py" % tag)
    path.write_text(source, encoding="utf-8")
    spec = importlib.util.spec_from_file_location("adequacy_%s" % tag, path)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def collect(fn, vectors: list, arg_keys: list, result: dict) -> None:
    """One entry per vector, keyed by position rather than by vector id.

    JSON object keys are strings. A corpus whose ids are integers would come
    back re-typed, and two ids differing only in type would collide.
    """
    for index, vector in enumerate(vectors):
        key = str(index)
        try:
            value = fn(*[vector[k] for k in arg_keys])
        except Exception:  # noqa: BLE001 - any raise is the signal, not an error
            # SystemExit and KeyboardInterrupt must escape: they are
            # process-control, not application errors. The parent
            # role-classifies the dead child (control-error for a control,
            # unexpected-exit/killed for a scored mutant).
            result["raised"].append(key)
            continue
        try:
            snapshot = json.loads(json.dumps(value, allow_nan=False))
        except (TypeError, ValueError):
            result["unsupported"].append(key)
            continue
        result["outcomes"][key] = snapshot


def restore_path(paths: list) -> None:
    """Import the corpus the way the parent would have.

    An implementation may import siblings, and a child starting from a
    different sys.path would measure a different program than the one the
    manifest names.
    """
    declared = [p for p in paths if isinstance(p, str)]
    sys.path[:] = declared + [p for p in sys.path if p not in set(declared)]


def emit(fd: int, result: dict) -> int:
    with os.fdopen(fd, "w", encoding="utf-8") as stream:
        json.dump(result, stream, allow_nan=False)
    return 0


def main(argv: list) -> int:
    request = json.loads(Path(argv[1]).read_text(encoding="utf-8"))
    protocol_fd = os.dup(1)
    os.dup2(2, 1)          # every candidate write to fd 1 lands on bounded stderr
    result = {"schema": SCHEMA, "outcomes": {}, "raised": [], "unsupported": [],
              "load_error": None, "entrypoint_missing": False}
    restore_path(request.get("sys_path") or [])
    try:
        module = load_module(request["source"], request["tag"],
                             Path(request["work_dir"]))
    except BaseException as exc:  # noqa: BLE001 - SystemExit at import is a load failure
        result["load_error"] = repr(exc)
        return emit(protocol_fd, result)
    fn = getattr(module, request["entrypoint"], None)
    if fn is None:
        result["entrypoint_missing"] = True
        return emit(protocol_fd, result)
    collect(fn, request["vectors"], request["arg_keys"], result)
    return emit(protocol_fd, result)


if __name__ == "__main__":
    sys.exit(main(sys.argv))
