#!/usr/bin/env python3
"""Stage sha256-verified assay-mcp-server binaries for the GHCR image.

The image copies these files; it does not rebuild them. A binary whose highest
GLIBC_ requirement exceeds Debian 13's 2.41 is refused so ubuntu-latest cannot
publish an image that fails at startup.
"""
from __future__ import annotations

import argparse
import hashlib
import stat
import sys
import tarfile
from pathlib import Path

SHT_GNU_VERNEED = 0x6FFFFFFE
ELFCLASS64 = 2
ELFDATA2LSB = 1

TARGET_TO_DOCKER_ARCH = {
    "x86_64-unknown-linux-gnu": "amd64",
    "aarch64-unknown-linux-gnu": "arm64",
}

REQUIRED_TARGETS = tuple(TARGET_TO_DOCKER_ARCH)
MAX_GLIBC = (2, 41)
BINARY_NAME = "assay-mcp-server"


def _die(message: str) -> None:
    raise SystemExit(message)


def parse_glibc_version(name: str) -> tuple[int, int] | None:
    if not name.startswith("GLIBC_"):
        return None
    rest = name[len("GLIBC_") :]
    parts = rest.split(".")
    if len(parts) < 2 or not all(part.isdigit() for part in parts[:2]):
        return None
    return (int(parts[0]), int(parts[1]))


def _u16(data: bytes, offset: int, little: bool) -> int:
    return int.from_bytes(data[offset : offset + 2], "little" if little else "big")


def _u32(data: bytes, offset: int, little: bool) -> int:
    return int.from_bytes(data[offset : offset + 4], "little" if little else "big")


def _u64(data: bytes, offset: int, little: bool) -> int:
    return int.from_bytes(data[offset : offset + 8], "little" if little else "big")


def glibc_versions_from_elf(payload: bytes) -> list[tuple[int, int]]:
    if len(payload) < 64 or payload[:4] != b"\x7fELF":
        _die("staged file is not an ELF binary")
    ident = payload[:16]
    cls = ident[4]
    data = ident[5]
    if cls != ELFCLASS64:
        _die("staged binary is not ELF64")
    little = data == ELFDATA2LSB
    e_shoff = _u64(payload, 40, little)
    e_shentsize = _u16(payload, 58, little)
    e_shnum = _u16(payload, 60, little)
    if e_shentsize == 0 or e_shnum == 0:
        _die("ELF has no section headers; cannot read GLIBC_ requirements")
    versions: list[tuple[int, int]] = []
    for index in range(e_shnum):
        sh_off = e_shoff + index * e_shentsize
        shdr = payload[sh_off : sh_off + e_shentsize]
        if len(shdr) < 64:
            _die("truncated ELF section header")
        sh_type = _u32(shdr, 4, little)
        if sh_type != SHT_GNU_VERNEED:
            continue
        sh_offset = _u64(shdr, 24, little)
        sh_size = _u64(shdr, 32, little)
        sh_link = _u32(shdr, 40, little)
        strtab_hdr_off = e_shoff + sh_link * e_shentsize
        strtab_hdr = payload[strtab_hdr_off : strtab_hdr_off + e_shentsize]
        if len(strtab_hdr) < 64:
            _die("truncated ELF string table header")
        str_off = _u64(strtab_hdr, 24, little)
        str_size = _u64(strtab_hdr, 32, little)
        dynstr = payload[str_off : str_off + str_size]
        cursor = 0
        verneed = payload[sh_offset : sh_offset + sh_size]
        while cursor + 16 <= len(verneed):
            vn_cnt = _u16(verneed, cursor + 2, little)
            vn_aux = _u32(verneed, cursor + 8, little)
            vn_next = _u32(verneed, cursor + 12, little)
            aux = cursor + vn_aux
            for _ in range(vn_cnt):
                if aux + 16 > len(verneed):
                    _die("truncated GNU verneed auxiliary entry")
                vna_name = _u32(verneed, aux + 8, little)
                vna_next = _u32(verneed, aux + 12, little)
                name = dynstr[vna_name:].split(b"\0", 1)[0].decode("ascii", "replace")
                parsed = parse_glibc_version(name)
                if parsed is not None:
                    versions.append(parsed)
                if vna_next == 0:
                    break
                aux += vna_next
            if vn_next == 0:
                break
            cursor += vn_next
    return versions


def check_glibc(payload: bytes, max_glibc: tuple[int, int]) -> tuple[int, int]:
    versions = glibc_versions_from_elf(payload)
    if not versions:
        _die("ELF declares no GLIBC_ version requirements")
    highest = max(versions)
    if highest > max_glibc:
        _die(
            f"GLIBC_{highest[0]}.{highest[1]} exceeds "
            f"GLIBC_{max_glibc[0]}.{max_glibc[1]}"
        )
    return highest


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def verify_sidecar(archive: Path) -> None:
    sidecar = Path(str(archive) + ".sha256")
    if not sidecar.is_file():
        _die(f"missing checksum sidecar: {sidecar.name}")
    line = sidecar.read_text(encoding="utf-8").splitlines()
    if len(line) != 1:
        _die(f"checksum sidecar must contain exactly one line: {sidecar.name}")
    parts = line[0].split()
    if len(parts) != 2:
        _die(f"checksum sidecar is not 'digest  filename': {sidecar.name}")
    expected, name = parts
    if name != archive.name:
        _die(f"checksum sidecar names {name!r}, want {archive.name!r}")
    actual = sha256_file(archive)
    if actual != expected:
        _die(f"sha256 mismatch for {archive.name}")


def extract_binary(archive: Path) -> bytes:
    with tarfile.open(archive, "r:gz") as tar:
        members = [
            member
            for member in tar.getmembers()
            if member.isfile() and Path(member.name).name == BINARY_NAME
        ]
        if len(members) != 1:
            _die(
                f"{archive.name}: want exactly one {BINARY_NAME} member, "
                f"found {len(members)}"
            )
        extracted = tar.extractfile(members[0])
        if extracted is None:
            _die(f"{archive.name}: {BINARY_NAME} member is not readable")
        payload = extracted.read()
    if not payload:
        _die(f"{archive.name}: {BINARY_NAME} is empty")
    return payload


def target_from_archive_name(name: str) -> str:
    for target in REQUIRED_TARGETS:
        if name.endswith(f"-{target}.tar.gz"):
            return target
    _die(f"unrecognized linux mcp-server archive: {name}")
    raise AssertionError("unreachable")


def write_binary(dest: Path, arch: str, payload: bytes) -> Path:
    placed = dest / arch / BINARY_NAME
    placed.parent.mkdir(parents=True, exist_ok=True)
    placed.write_bytes(payload)
    placed.chmod(placed.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)
    return placed


def stage_binary(
    binary: Path,
    dest: Path,
    *,
    arch: str,
    max_glibc: tuple[int, int] = MAX_GLIBC,
) -> Path:
    if arch not in TARGET_TO_DOCKER_ARCH.values():
        _die(f"unknown docker arch {arch!r}")
    if not binary.is_file():
        _die(f"binary is not a file: {binary}")
    payload = binary.read_bytes()
    check_glibc(payload, max_glibc)
    return write_binary(dest, arch, payload)


def stage(
    archives_dir: Path,
    dest: Path,
    *,
    max_glibc: tuple[int, int] = MAX_GLIBC,
) -> dict[str, Path]:
    if not archives_dir.is_dir():
        _die(f"archives directory not found: {archives_dir}")
    placed: dict[str, Path] = {}
    found: dict[str, Path] = {}
    for archive in sorted(archives_dir.iterdir()):
        if not archive.is_file() or not archive.name.endswith(".tar.gz"):
            continue
        if not archive.name.startswith(f"{BINARY_NAME}-"):
            continue
        target = target_from_archive_name(archive.name)
        found[target] = archive
    missing = [target for target in REQUIRED_TARGETS if target not in found]
    if missing:
        _die("missing linux mcp-server archives: " + ", ".join(missing))
    for target, archive in found.items():
        verify_sidecar(archive)
        payload = extract_binary(archive)
        check_glibc(payload, max_glibc)
        arch = TARGET_TO_DOCKER_ARCH[target]
        placed[arch] = write_binary(dest, arch, payload)
    return placed


def parse_max_glibc(value: str) -> tuple[int, int]:
    raw = value if value.startswith("GLIBC_") else "GLIBC_" + value
    parsed = parse_glibc_version(raw)
    if parsed is None:
        _die(f"invalid GLIBC ceiling {value!r}")
    return parsed


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--archives-dir", type=Path)
    parser.add_argument("--binary", type=Path)
    parser.add_argument("--arch", choices=sorted(set(TARGET_TO_DOCKER_ARCH.values())))
    parser.add_argument("--output-dir", type=Path, required=True)
    parser.add_argument("--max-glibc", default="2.41")
    args = parser.parse_args(argv)
    max_glibc = parse_max_glibc(args.max_glibc)
    if args.archives_dir is not None:
        if args.binary is not None or args.arch is not None:
            _die("use either --archives-dir or --binary/--arch")
        placed = stage(args.archives_dir, args.output_dir, max_glibc=max_glibc)
        for arch, path in sorted(placed.items()):
            print(f"ok    staged {arch} -> {path}")
        return 0
    if args.binary is None or args.arch is None:
        _die("provide --archives-dir, or both --binary and --arch")
    path = stage_binary(
        args.binary, args.output_dir, arch=args.arch, max_glibc=max_glibc
    )
    print(f"ok    staged {args.arch} -> {path}")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except SystemExit as error:
        if isinstance(error.code, str):
            print(error.code, file=sys.stderr)
            raise SystemExit(1) from error
        raise
