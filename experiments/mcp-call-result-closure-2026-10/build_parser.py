"""Materialize pinned, unmodified Assay modules in an owned temporary build directory."""

import hashlib
import json
import re
import shutil
import subprocess
import sys
from pathlib import Path

PIN = "01dd1006cefccc4f0fb2d4fd86a307939e10eb69"
MODULES = ("parser", "era", "types", "identity", "jcs", "json_depth")


def resource_gate():
    """Admission check on this Mac; not a continuous memory/disk sandbox."""
    swap = subprocess.check_output(["sysctl", "vm.swapusage"], text=True)
    match = re.search(r"total = ([\d.]+)M.*free = ([\d.]+)M", swap)
    if not match or float(match[2]) / float(match[1]) < 0.05:
        raise RuntimeError("free swap below 5% or unavailable")
    if shutil.disk_usage("/System/Volumes/Data").free < 5_000_000_000:
        raise RuntimeError("free disk below 5 GB")
    return swap.strip()


def build(repo, dest):
    resource_gate()
    names = subprocess.check_output(["ps", "-axo", "ucomm="], text=True).splitlines()
    if any(n.strip() in ("cargo", "rustc", "clippy") for n in names):
        raise RuntimeError("another compile lane is active")
    dest = Path(dest)
    dest.mkdir(exist_ok=False)
    module = dest / "src/mcp"
    module.mkdir(parents=True)
    hashes = {}
    for name in MODULES:
        path = f"crates/assay-core/src/mcp/{name}.rs"
        raw = subprocess.check_output(["git", "-C", repo, "show", f"{PIN}:{path}"])
        (module / (name + ".rs")).write_bytes(raw)
        hashes[path] = hashlib.sha256(raw).hexdigest()
    main = Path(__file__).with_name("parser_main.rs").read_bytes()
    (dest / "src/main.rs").write_bytes(main)
    (dest / "Cargo.toml").write_text("""[package]
name = "closure-parser-probe"
version = "0.0.0"
edition = "2021"
[dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
sha2 = "0.11"
serde_jcs = "=0.2.0"
thiserror = "2"
hex = "0.4"
""")
    (dest / "Cargo.lock").write_bytes(
        Path(__file__).with_name("Cargo.lock").read_bytes()
    )
    subprocess.run(
        [
            "cargo",
            "build",
            "--locked",
            "--offline",
            "-j1",
            "--manifest-path",
            str(dest / "Cargo.toml"),
        ],
        check=True,
        timeout=180,
    )
    binary = dest / "target/debug/closure-parser-probe"
    receipt = {
        "assay_commit": PIN,
        "rustc": subprocess.check_output(
            ["rustc", "--version", "--verbose"], text=True
        ),
        "features": subprocess.check_output(
            [
                "cargo",
                "tree",
                "--locked",
                "--offline",
                "-e",
                "features",
                "--manifest-path",
                str(dest / "Cargo.toml"),
            ],
            text=True,
        ),
        "modules": hashes,
        "wrapper_sha256": hashlib.sha256(main).hexdigest(),
        "lock_sha256": hashlib.sha256((dest / "Cargo.lock").read_bytes()).hexdigest(),
        "binary_sha256": hashlib.sha256(binary.read_bytes()).hexdigest(),
    }
    (dest / "source-receipt.json").write_text(json.dumps(receipt, indent=2) + "\n")
    print(binary)


if __name__ == "__main__":
    build(sys.argv[1], sys.argv[2])
