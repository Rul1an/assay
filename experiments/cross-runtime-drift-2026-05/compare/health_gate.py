#!/usr/bin/env python3
"""Hard gate on Runner observation-health.

Reads observation-health.json from a Runner archive (directory or
.tar.gz) and exits non-zero if any of the three completeness
invariants the experiment relies on is violated:

  - ringbuf_drops != 0       (kernel events were dropped)
  - kernel_layer != complete (kernel capture incomplete)
  - cgroup_correlation != clean (cgroup v2 correlation broke)

Used by the cross-runtime-drift-experiment workflow per iteration,
before any artifact upload. The plan-doc's discipline says to discard
runs with degraded health; this script enforces it instead of relying
on the maintainer remembering during baseline commit (P2 review on
PR #1347).

Exit codes:
  0 - all three gates passed
  2 - bad CLI args / I/O
  3 - bad archive (missing manifest/health, corrupt JSON, etc.)
  4 - one or more gates failed (details on stderr)
"""
from __future__ import annotations

import argparse
import json
import sys
import tarfile
from pathlib import Path
from typing import Any

OBSERVATION_HEALTH_PATH = "observation-health.json"


def _read_member(source: Path, member: str) -> bytes:
    if source.is_dir():
        path = source / member
        if not path.is_file():
            raise FileNotFoundError(f"{source}!{member}: not found")
        return path.read_bytes()
    with tarfile.open(source, "r:*") as tf:
        try:
            extracted = tf.extractfile(member)
        except KeyError as exc:
            raise FileNotFoundError(
                f"{source}!{member}: not in archive"
            ) from exc
        if extracted is None:
            raise FileNotFoundError(f"{source}!{member}: not a regular file")
        return extracted.read()


def evaluate_health(health: dict[str, Any]) -> list[str]:
    """Return a list of failure descriptions; empty list means PASS.

    `ringbuf_drops` is a required health invariant: missing, null, or
    non-int values are a failure (not silently treated as 0). Same for
    the other two: a missing field is *not* assumed to mean "clean"."""
    issues: list[str] = []
    if "ringbuf_drops" not in health:
        issues.append("ringbuf_drops=<missing>")
    else:
        drops = health["ringbuf_drops"]
        if not isinstance(drops, int) or isinstance(drops, bool) or drops != 0:
            issues.append(f"ringbuf_drops={drops!r}")
    if "kernel_layer" not in health:
        issues.append("kernel_layer=<missing>")
    elif health["kernel_layer"] != "complete":
        issues.append(f"kernel_layer={health['kernel_layer']!r}")
    if "cgroup_correlation" not in health:
        issues.append("cgroup_correlation=<missing>")
    elif health["cgroup_correlation"] != "clean":
        issues.append(f"cgroup_correlation={health['cgroup_correlation']!r}")
    return issues


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--archive",
        required=True,
        type=Path,
        help="Path to the Runner archive (directory or .tar.gz).",
    )
    args = parser.parse_args(argv)

    archive: Path = args.archive
    if not archive.exists():
        print(f"archive does not exist: {archive}", file=sys.stderr)
        return 3
    try:
        data = _read_member(archive, OBSERVATION_HEALTH_PATH)
    except FileNotFoundError as exc:
        print(f"bad archive: {exc}", file=sys.stderr)
        return 3
    except (tarfile.TarError, OSError) as exc:
        print(f"bad archive: {archive}: {exc}", file=sys.stderr)
        return 3
    try:
        health = json.loads(data.decode("utf-8"))
    except (json.JSONDecodeError, UnicodeDecodeError) as exc:
        print(
            f"bad archive: {archive}!{OBSERVATION_HEALTH_PATH}: {exc}",
            file=sys.stderr,
        )
        return 3
    if not isinstance(health, dict):
        print(
            f"bad archive: {archive}!{OBSERVATION_HEALTH_PATH}: "
            f"not a JSON object",
            file=sys.stderr,
        )
        return 3

    issues = evaluate_health(health)
    if issues:
        print(
            f"HEALTH GATE FAIL ({archive}): {', '.join(issues)}",
            file=sys.stderr,
        )
        return 4
    print(f"HEALTH GATE PASS ({archive})")
    return 0


if __name__ == "__main__":
    sys.exit(main())
