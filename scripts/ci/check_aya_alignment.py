"""Check the Aya compatibility line used by the monitor and its kernel object."""
import re
import sys
import tomllib
from pathlib import Path


def read_manifest(path):
    with path.open("rb") as stream:
        return tomllib.load(stream)


def version_line(value, prefix):
    match = re.fullmatch(prefix + r"(\d+\.\d+)(?:\.\d+)?", value)
    if not match:
        raise ValueError(f"unsupported Aya version: {value!r}")
    return match[1]


def check(root):
    workspace = read_manifest(root / "Cargo.toml")["workspace"]["dependencies"]
    kernel = read_manifest(root / "crates/assay-ebpf/Cargo.toml")["dependencies"]
    tag = kernel["aya-ebpf"]["tag"]
    if tag != kernel["aya-log-ebpf"]["tag"]:
        raise ValueError("aya-ebpf and aya-log-ebpf must use the same git tag")

    line = version_line(tag, "aya-v")
    if version_line(workspace["aya"], "=?") != line:
        raise ValueError(f"workspace aya must align with kernel tag {tag}")
    log_lines = {"0.13": "0.2", "0.14": "0.3", "0.16": "0.4"}
    if line not in log_lines:
        raise ValueError(f"unknown Aya compatibility line {line}")
    if version_line(workspace["aya-log"], "=?") != log_lines[line]:
        raise ValueError(f"workspace aya-log must be {log_lines[line]} for aya {line}")
    print(f"PASS: workspace Aya dependencies align with {tag}")


if __name__ == "__main__":
    try:
        check(Path(__file__).resolve().parents[2])
    except (KeyError, OSError, TypeError, ValueError) as err:
        sys.exit(f"FAIL: {err}")
