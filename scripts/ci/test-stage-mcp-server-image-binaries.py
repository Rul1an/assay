#!/usr/bin/env python3
"""Stage-script contract: verified sidecars, per-arch layout, GLIBC ceiling."""
from __future__ import annotations

import hashlib
import importlib.util
import io
import struct
import tarfile
import unittest
from pathlib import Path
from tempfile import TemporaryDirectory

REPO = Path(__file__).resolve().parents[2]
SCRIPT = REPO / "scripts/ci/stage-mcp-server-image-binaries.py"

SHT_NULL = 0
SHT_STRTAB = 3
SHT_GNU_VERNEED = 0x6FFFFFFE
ELF64_HEADER_SIZE = 64
ELF64_SHDR_SIZE = 64


def elf_hash(name: str) -> int:
    h = 0
    for byte in name.encode("ascii"):
        h = (h << 4) + byte
        g = h & 0xF0000000
        if g:
            h ^= g >> 24
        h &= ~g
        h &= 0xFFFFFFFF
    return h


def gnu_verneed_elf(glibc_version: str) -> bytes:
    """Minimal little-endian ELF64 with one GLIBC_ verneed (test fixture)."""
    dynstr = b"\0libc.so.6\0" + glibc_version.encode("ascii") + b"\0"
    libc_off = 1
    ver_off = 1 + len(b"libc.so.6") + 1
    verneed = struct.pack("<HHIII", 1, 1, libc_off, 16, 0) + struct.pack(
        "<IHHII", elf_hash(glibc_version), 0, 1, ver_off, 0
    )
    shstrtab = b"\0.dynstr\0.gnu.version_r\0.shstrtab\0"
    dynstr_name = 1
    verneed_name = 1 + len(b".dynstr") + 1
    shstrtab_name = verneed_name + len(b".gnu.version_r") + 1

    dynstr_off = ELF64_HEADER_SIZE
    verneed_off = dynstr_off + len(dynstr)
    shstrtab_off = verneed_off + len(verneed)
    shoff = shstrtab_off + len(shstrtab)

    def shdr(
        name: int,
        sh_type: int,
        offset: int,
        size: int,
        link: int = 0,
        info: int = 0,
        entsize: int = 0,
    ) -> bytes:
        return struct.pack(
            "<IIQQQQIIQQ",
            name,
            sh_type,
            0,
            0,
            offset,
            size,
            link,
            info,
            1,
            entsize,
        )

    headers = (
        shdr(0, SHT_NULL, 0, 0)
        + shdr(dynstr_name, SHT_STRTAB, dynstr_off, len(dynstr))
        + shdr(
            verneed_name,
            SHT_GNU_VERNEED,
            verneed_off,
            len(verneed),
            link=1,
            info=1,
            entsize=16,
        )
        + shdr(shstrtab_name, SHT_STRTAB, shstrtab_off, len(shstrtab))
    )
    ident = bytearray(16)
    ident[0:4] = b"\x7fELF"
    ident[4] = 2  # ELFCLASS64
    ident[5] = 1  # ELFDATA2LSB
    ident[6] = 1
    header = struct.pack(
        "<16sHHIQQQIHHHHHH",
        bytes(ident),
        3,  # ET_DYN
        62,  # EM_X86_64
        1,
        0,
        0,
        shoff,
        0,
        ELF64_HEADER_SIZE,
        0,
        0,
        ELF64_SHDR_SIZE,
        4,
        3,
    )
    return header + dynstr + verneed + shstrtab + headers


def load_script():
    spec = importlib.util.spec_from_file_location("stage_mcp_server_image_binaries", SCRIPT)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load {SCRIPT}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def write_archive(directory: Path, target: str, payload: bytes) -> Path:
    archive_name = f"assay-mcp-server-v9.9.9-{target}.tar.gz"
    archive_path = directory / archive_name
    buf = io.BytesIO()
    with tarfile.open(fileobj=buf, mode="w:gz") as tar:
        info = tarfile.TarInfo(f"assay-mcp-server-v9.9.9-{target}/assay-mcp-server")
        info.size = len(payload)
        info.mode = 0o755
        tar.addfile(info, io.BytesIO(payload))
    data = buf.getvalue()
    archive_path.write_bytes(data)
    digest = hashlib.sha256(data).hexdigest()
    archive_path.with_suffix(archive_path.suffix + ".sha256").write_text(
        f"{digest}  {archive_name}\n", encoding="utf-8"
    )
    return archive_path


class StageMcpServerImageBinaries(unittest.TestCase):
    @classmethod
    def setUpClass(cls) -> None:
        if not SCRIPT.is_file():
            raise AssertionError(f"missing {SCRIPT}")
        cls.mod = load_script()

    def test_verified_linux_archives_are_placed_per_docker_arch(self) -> None:
        with TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            dest = Path(tmp) / "binaries"
            src.mkdir()
            allowed = gnu_verneed_elf("GLIBC_2.39")
            write_archive(src, "x86_64-unknown-linux-gnu", allowed)
            write_archive(src, "aarch64-unknown-linux-gnu", allowed)
            self.mod.stage(src, dest, max_glibc=(2, 41))
            for arch in ("amd64", "arm64"):
                placed = dest / arch / "assay-mcp-server"
                self.assertTrue(placed.is_file(), arch)
                self.assertEqual(placed.read_bytes(), allowed)
                self.assertTrue(placed.stat().st_mode & 0o111)

    def test_glibc_newer_than_debian13_must_bite(self) -> None:
        with TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            dest = Path(tmp) / "binaries"
            src.mkdir()
            too_new = gnu_verneed_elf("GLIBC_2.42")
            write_archive(src, "x86_64-unknown-linux-gnu", too_new)
            write_archive(src, "aarch64-unknown-linux-gnu", gnu_verneed_elf("GLIBC_2.39"))
            with self.assertRaises(SystemExit) as raised:
                self.mod.stage(src, dest, max_glibc=(2, 41))
            self.assertIn("GLIBC_2.42", str(raised.exception))
            self.assertFalse((dest / "amd64" / "assay-mcp-server").exists())

    def test_glibc_equal_to_ceiling_is_admitted(self) -> None:
        with TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            dest = Path(tmp) / "binaries"
            src.mkdir()
            ceiling = gnu_verneed_elf("GLIBC_2.41")
            write_archive(src, "x86_64-unknown-linux-gnu", ceiling)
            write_archive(src, "aarch64-unknown-linux-gnu", ceiling)
            self.mod.stage(src, dest, max_glibc=(2, 41))
            self.assertTrue((dest / "amd64" / "assay-mcp-server").is_file())

    def test_tampered_sidecar_is_refused(self) -> None:
        with TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            dest = Path(tmp) / "binaries"
            src.mkdir()
            payload = gnu_verneed_elf("GLIBC_2.39")
            archive = write_archive(src, "x86_64-unknown-linux-gnu", payload)
            write_archive(src, "aarch64-unknown-linux-gnu", payload)
            sidecar = archive.with_suffix(archive.suffix + ".sha256")
            sidecar.write_text(
                "0" * 64 + "  " + archive.name + "\n", encoding="utf-8"
            )
            with self.assertRaises(SystemExit):
                self.mod.stage(src, dest, max_glibc=(2, 41))

    def test_missing_sidecar_is_refused(self) -> None:
        with TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            dest = Path(tmp) / "binaries"
            src.mkdir()
            payload = gnu_verneed_elf("GLIBC_2.39")
            write_archive(src, "x86_64-unknown-linux-gnu", payload)
            write_archive(src, "aarch64-unknown-linux-gnu", payload)
            (src / "assay-mcp-server-v9.9.9-x86_64-unknown-linux-gnu.tar.gz.sha256").unlink()
            with self.assertRaises(SystemExit):
                self.mod.stage(src, dest, max_glibc=(2, 41))

    def test_unrelated_tarball_in_archives_dir_is_ignored(self) -> None:
        with TemporaryDirectory() as tmp:
            src = Path(tmp) / "src"
            dest = Path(tmp) / "binaries"
            src.mkdir()
            allowed = gnu_verneed_elf("GLIBC_2.39")
            write_archive(src, "x86_64-unknown-linux-gnu", allowed)
            write_archive(src, "aarch64-unknown-linux-gnu", allowed)
            other = src / "assay-v9.9.9-sbom-cyclonedx.tar.gz"
            other.write_bytes(b"not-a-binary-archive")
            self.mod.stage(src, dest, max_glibc=(2, 41))
            self.assertTrue((dest / "amd64" / "assay-mcp-server").is_file())

    def test_single_binary_path_still_enforces_glibc(self) -> None:
        with TemporaryDirectory() as tmp:
            dest = Path(tmp) / "binaries"
            too_new = Path(tmp) / "assay-mcp-server"
            too_new.write_bytes(gnu_verneed_elf("GLIBC_2.42"))
            too_new.chmod(0o755)
            with self.assertRaises(SystemExit) as raised:
                self.mod.stage_binary(too_new, dest, arch="amd64", max_glibc=(2, 41))
            self.assertIn("GLIBC_2.42", str(raised.exception))


if __name__ == "__main__":
    unittest.main(verbosity=2)
