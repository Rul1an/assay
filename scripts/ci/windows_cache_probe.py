#!/usr/bin/env python3
"""Bounded cache observation for the Windows `test` leg (#3113).

Observability only. It reports what the rust-cache restore and sccache actually left behind, as
aggregates: no file names, no contents, no environment. Anything it could not observe is reported
as unknown (null) with a `::warning::`, never as zero, and it exits 0 so an observation failure
cannot change a test result.

`--cache-hit` is rust-cache's only output. `false` means no exact key match, which includes a
partial restore from a fallback key; it is not evidence of a cold cache.
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import threading
import time

SCHEMA = "assay.ci.windows_cache_probe.v0"
MAX_ENTRIES = 200_000
TIME_BUDGET_S = 60.0
SCCACHE_TIMEOUT_S = 30.0
SCCACHE_MAX_BYTES = 64 * 1024
MAX_DIRS = 4
MAX_FIELD = 64
LABEL = re.compile(r"[a-z0-9_-]{1,32}")
BACKEND = re.compile(r"[A-Za-z][A-Za-z0-9 _-]{0,31}")


class ProbeArgumentError(Exception):
    pass


class Parser(argparse.ArgumentParser):
    def error(self, message: str) -> None:  # never exit non-zero on bad arguments
        raise ProbeArgumentError(message)


def short(value: object) -> str | None:
    if not isinstance(value, str):
        return None
    return value[:MAX_FIELD]


def walk(root: str, max_entries: int, deadline: float) -> dict:
    entry = {"status": None, "files": None, "dirs": None, "symlinks": None, "bytes": None,
             "lower_bound": False, "errors": None}
    try:
        if not os.path.lexists(root):
            entry["status"] = "absent"
            return entry
        if not os.path.isdir(root) or os.path.islink(root):
            entry["status"] = "not_a_directory"
            return entry
        stack = [os.scandir(root)]
    except OSError:
        entry["status"] = "unreadable"
        return entry

    files = dirs = links = size = seen = errors = 0
    truncated = False
    while stack:
        it = stack[-1]
        if seen >= max_entries or time.monotonic() >= deadline:
            truncated = True
            break
        try:
            item = next(it, None)
        except OSError:
            errors += 1
            item = None
        if item is None:
            it.close()
            stack.pop()
            continue
        seen += 1
        try:
            if item.is_symlink():
                links += 1
            elif item.is_dir(follow_symlinks=False):
                dirs += 1
                try:
                    stack.append(os.scandir(item.path))
                except OSError:
                    errors += 1
            else:
                files += 1
                size += item.stat(follow_symlinks=False).st_size
        except OSError:
            errors += 1
    for it in stack:
        it.close()

    entry.update(files=files, dirs=dirs, symlinks=links, bytes=size, errors=errors)
    if truncated:
        entry.update(status="truncated", lower_bound=True)
    elif errors:
        entry.update(status="partial", lower_bound=True)
    else:
        entry["status"] = "empty" if seen == 0 else "present"
    return entry


def read_capped(stream, limit: int, out: list) -> None:
    data = b""
    while len(data) <= limit:
        chunk = stream.read(min(8192, limit + 1 - len(data)))
        if not chunk:
            break
        data += chunk
    out.append(data)


def counter(stats: dict, key: str) -> int | None:
    value = stats.get(key)
    if isinstance(value, dict):
        counts = value.get("counts")
        if isinstance(counts, dict) and all(isinstance(v, int) for v in counts.values()):
            return sum(counts.values())
        return None
    return value if isinstance(value, int) and not isinstance(value, bool) else None


def backend_kind(location: object) -> str | None:
    # "Local disk: \"C:\\...\"" -> "Local disk". The path after the colon is never reported.
    if not isinstance(location, str) or ":" not in location:
        return None
    kind = location.split(":", 1)[0].strip()
    return kind if BACKEND.fullmatch(kind) else None


def sccache_stats(command: list[str], timeout: float) -> dict:
    result = {"status": None, "hits": None, "misses": None, "compile_requests": None,
              "cache_size_bytes": None, "backend": None, "version": None}
    try:
        proc = subprocess.Popen(
            [*command, "--show-stats", "--stats-format", "json"],
            stdin=subprocess.DEVNULL, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL,
        )
    except OSError:
        result["status"] = "missing"
        return result
    out: list[bytes] = []
    reader = threading.Thread(target=read_capped, args=(proc.stdout, SCCACHE_MAX_BYTES, out),
                              daemon=True)
    reader.start()
    reader.join(timeout)
    if reader.is_alive():
        proc.kill()
        proc.wait()
        result["status"] = "timeout"
        return result
    data = out[0] if out else b""
    if len(data) > SCCACHE_MAX_BYTES:
        proc.kill()
        proc.wait()
        result["status"] = "oversized"
        return result
    try:
        code = proc.wait(timeout=max(1.0, timeout))
    except subprocess.TimeoutExpired:
        proc.kill()
        proc.wait()
        result["status"] = "timeout"
        return result
    if code != 0:
        result["status"] = "nonzero_exit"
        return result
    try:
        info = json.loads(data.decode("utf-8"))
    except (UnicodeDecodeError, ValueError):
        result["status"] = "malformed"
        return result
    if not isinstance(info, dict):
        result["status"] = "malformed"
        return result
    stats = info.get("stats") if isinstance(info.get("stats"), dict) else {}
    location = info.get("cache_location")
    size = info.get("cache_size")
    result.update(
        status="ok",
        hits=counter(stats, "cache_hits"),
        misses=counter(stats, "cache_misses"),
        compile_requests=counter(stats, "compile_requests"),
        cache_size_bytes=size if isinstance(size, int) and not isinstance(size, bool) else None,
        backend=backend_kind(location),
        version=short(info.get("version")),
    )
    return result


def summary_lines(report: dict) -> list[str]:
    def cell(value: object) -> str:
        return "unknown" if value is None else str(value)

    lines = [
        f"### Windows cache probe: {report['phase']}",
        "",
        f"rust-cache exact key hit: `{report['rust_cache_exact_hit']}`"
        " (false = no exact match, not proof of a cold cache)",
        "",
        "| dir | status | files | dirs | bytes | lower bound |",
        "|---|---|---|---|---|---|",
    ]
    for label, d in report["dirs"].items():
        lines.append(f"| {label} | {d['status']} | {cell(d['files'])} | {cell(d['dirs'])} | "
                     f"{cell(d['bytes'])} | {d['lower_bound']} |")
    s = report["sccache"]
    lines += ["", f"sccache: status `{s['status']}`, hits {cell(s['hits'])}, misses "
              f"{cell(s['misses'])}, requests {cell(s['compile_requests'])}, cache bytes "
              f"{cell(s['cache_size_bytes'])}, backend {cell(s['backend'])}, version "
              f"{cell(s['version'])}", ""]
    return lines


def main(argv: list[str]) -> int:
    warnings: list[str] = []
    report: dict = {
        "schema": SCHEMA, "phase": "unknown", "rust_cache_exact_hit": "unknown", "dirs": {},
        "sccache": {"status": "not_run", "hits": None, "misses": None, "compile_requests": None,
                    "cache_size_bytes": None, "backend": None, "version": None},
        "limits": {}, "warnings": warnings,
    }
    parser = Parser(add_help=False)
    parser.add_argument("--phase", choices=("pre", "post"))
    parser.add_argument("--cache-hit", default="")
    parser.add_argument("--dir", action="append", default=[])
    parser.add_argument("--sccache", action="append", default=[])
    parser.add_argument("--max-entries", type=int, default=MAX_ENTRIES)
    parser.add_argument("--time-budget", type=float, default=TIME_BUDGET_S)
    parser.add_argument("--sccache-timeout", type=float, default=SCCACHE_TIMEOUT_S)
    try:
        args = parser.parse_args(argv)
    except ProbeArgumentError as exc:
        warnings.append(f"bad arguments: {short(str(exc))}")
        return emit(report)

    report["phase"] = args.phase or "unknown"
    report["rust_cache_exact_hit"] = args.cache_hit if args.cache_hit in ("true", "false") else "unknown"

    limits = {"max_entries": args.max_entries, "time_budget_s": args.time_budget,
              "sccache_timeout_s": args.sccache_timeout}
    for key, ceiling in (("max_entries", MAX_ENTRIES), ("time_budget_s", TIME_BUDGET_S),
                         ("sccache_timeout_s", SCCACHE_TIMEOUT_S)):
        if not 0 <= limits[key] <= ceiling:
            warnings.append(f"limit {key} out of range; using {ceiling}")
            limits[key] = ceiling
    report["limits"] = limits

    deadline = time.monotonic() + limits["time_budget_s"]
    for spec in args.dir[:MAX_DIRS]:
        label, sep, path = spec.partition("=")
        if not sep or not LABEL.fullmatch(label) or not path:
            warnings.append("bad --dir argument; expected label=path with label [a-z0-9_-]")
            continue
        report["dirs"][label] = walk(path, limits["max_entries"], deadline)
    if len(args.dir) > MAX_DIRS:
        warnings.append(f"only the first {MAX_DIRS} --dir arguments were observed")

    if args.sccache:
        report["sccache"] = sccache_stats(args.sccache, limits["sccache_timeout_s"])
    for label, d in report["dirs"].items():
        if d["status"] not in ("present", "empty", "absent"):
            warnings.append(f"dir {label}: {d['status']}")
    if report["sccache"]["status"] != "ok":
        warnings.append(f"sccache stats: {report['sccache']['status']}")
    return emit(report)


def emit(report: dict) -> int:
    warnings = report["warnings"]
    summary = os.environ.get("GITHUB_STEP_SUMMARY")
    if summary:
        try:
            with open(summary, "a", encoding="utf-8") as fh:
                fh.write("\n".join(summary_lines(report)) + "\n")
        except OSError:
            warnings.append("job summary not writable")
    for warning in warnings:
        print(f"::warning::Windows cache probe ({report['phase']}): {warning}")
    print(json.dumps(report, sort_keys=True))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
