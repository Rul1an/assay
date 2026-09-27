#!/usr/bin/env python3
from __future__ import annotations

import io
import stat
import struct
import tarfile
import tempfile
import unittest
import zipfile
from pathlib import Path
from unittest import mock

from safe_extract_release_archive import ArchiveRejected, extract_archive


class SafeExtractReleaseArchiveTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def write_archive(self, rows: list[tuple[str, bytes, str]]) -> Path:
        archive = self.root / "input.tar.gz"
        with tarfile.open(archive, "w:gz") as handle:
            for name, payload, kind in rows:
                member = tarfile.TarInfo(name)
                if kind == "file":
                    member.size = len(payload)
                    member.mode = 0o755
                    handle.addfile(member, io.BytesIO(payload))
                elif kind == "symlink":
                    member.type = tarfile.SYMTYPE
                    member.linkname = payload.decode()
                    handle.addfile(member)
                else:
                    raise AssertionError(kind)
        return archive

    def test_extracts_bounded_regular_file(self) -> None:
        archive = self.write_archive([("pkg/assay", b"binary", "file")])
        destination = self.root / "out"

        extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertEqual((destination / "pkg/assay").read_bytes(), b"binary")
        self.assertTrue((destination / "pkg/assay").stat().st_mode & 0o100)

    def test_extracts_bounded_zip_regular_file(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as handle:
            handle.writestr("pkg/assay.exe", b"binary")
        destination = self.root / "out"

        extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertEqual((destination / "pkg/assay.exe").read_bytes(), b"binary")

    def test_rejects_zip_traversal_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("../escape", b"bad")
        destination = self.root / "out"

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertFalse(destination.exists())
        self.assertFalse((self.root / "escape").exists())

    def test_rejects_zip_windows_drive_path_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("C:/escape", b"bad")
        destination = self.root / "out"

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertFalse(destination.exists())

    def test_rejects_zip_windows_alias_names_before_materialization(self) -> None:
        names = (
            "pkg/NUL.txt",
            "pkg/NUL .txt",
            "pkg/COM1 .foo",
            "pkg/CONIN$",
            "pkg/CONOUT$.txt",
            "pkg/COM¹",
            "pkg/LPT³.foo",
            "pkg/bad?.txt",
            "pkg/control\x01.txt",
            "pkg/name.",
            "pkg/name ",
        )
        for index, name in enumerate(names):
            with self.subTest(name=name):
                archive = self.root / f"input-{index}.zip"
                with zipfile.ZipFile(archive, "w") as handle:
                    handle.writestr(name, b"bad")
                destination = self.root / f"out-{index}"

                with self.assertRaises(ArchiveRejected):
                    extract_archive(
                        archive,
                        destination,
                        max_decoded_bytes=32,
                        max_members=4,
                    )

                self.assertFalse(destination.exists())

    def test_rejects_zip_case_colliding_names_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("pkg/Assay.exe", b"one")
            handle.writestr("pkg/assay.exe", b"two")
        destination = self.root / "out"

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertFalse(destination.exists())

    def test_rejects_zip_link_member(self) -> None:
        archive = self.root / "input.zip"
        member = zipfile.ZipInfo("pkg/link")
        member.create_system = 3
        member.external_attr = (stat.S_IFLNK | 0o777) << 16
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr(member, "target")

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=32, max_members=4)

    def test_rejects_zip_decoded_size_ceiling(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as handle:
            handle.writestr("pkg/assay.exe", b"12345")

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=4, max_members=4)

    def test_rejects_zip_member_count_before_materializing_directory(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            for index in range(5):
                handle.writestr(f"pkg/{index}", b"")

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("central directory was materialized"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_zip_directory_bytes_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        member = zipfile.ZipInfo("pkg/assay.exe")
        member.comment = b"x" * 65535
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr(member, b"")

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("central directory was materialized"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=1,
                )

    def test_rejects_underreported_zip_member_count_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            for index in range(2):
                handle.writestr(f"pkg/{index}", b"")
        payload = bytearray(archive.read_bytes())
        eocd = payload.rfind(b"PK\x05\x06")
        self.assertGreaterEqual(eocd, 0)
        struct.pack_into("<HH", payload, eocd + 8, 1, 1)
        archive.write_bytes(payload)

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("central directory was materialized"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_raw_zip_name_before_zipfile_normalizes_it(self) -> None:
        archive = self.root / "input.zip"
        original_name = b"pkg/goodXevil"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr(original_name.decode(), b"bad")
        payload = archive.read_bytes()
        self.assertEqual(payload.count(original_name), 2)
        archive.write_bytes(payload.replace(original_name, b"pkg/good\x00evil"))

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("raw member name reached ZipFile"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_noncanonical_raw_zip_paths_before_materialization(self) -> None:
        for index, name in enumerate(("./pkg/file", "pkg//file", "pkg/./file")):
            with self.subTest(name=name):
                archive = self.root / f"input-{index}.zip"
                with zipfile.ZipFile(archive, "w") as handle:
                    handle.writestr(name, b"bad")
                with mock.patch.object(
                    zipfile,
                    "ZipFile",
                    side_effect=AssertionError("noncanonical name reached ZipFile"),
                ):
                    with self.assertRaises(ArchiveRejected):
                        extract_archive(
                            archive,
                            self.root / f"out-{index}",
                            max_decoded_bytes=32,
                            max_members=4,
                        )

    def test_rejects_unsupported_zip_codec_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("pkg/file", b"bad")
        payload = bytearray(archive.read_bytes())
        local = payload.find(b"PK\x03\x04")
        central = payload.find(b"PK\x01\x02")
        struct.pack_into("<H", payload, local + 8, zipfile.ZIP_LZMA)
        struct.pack_into("<H", payload, central + 10, zipfile.ZIP_LZMA)
        archive.write_bytes(payload)

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("unsupported codec reached ZipFile"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_zip64_local_record_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            member = zipfile.ZipInfo("pkg/file")
            with handle.open(member, "w", force_zip64=True) as output:
                output.write(b"bad")

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("ZIP64 record reached ZipFile"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_local_central_name_mismatch_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        local_name = b"pkg/good.exe"
        central_name = b"pkg/evil.exe"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr(local_name.decode(), b"bad")
        payload = bytearray(archive.read_bytes())
        self.assertEqual(bytes(payload).count(local_name), 2)
        local_offset = payload.find(local_name)
        payload[local_offset : local_offset + len(local_name)] = central_name
        archive.write_bytes(payload)

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("mismatched local name reached ZipFile"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_file_prefix_conflict_before_materialization(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("pkg", b"file")
            handle.writestr("pkg/child", b"child")

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=AssertionError("path conflict reached ZipFile"),
        ):
            with self.assertRaises(ArchiveRejected):
                extract_archive(
                    archive,
                    self.root / "out",
                    max_decoded_bytes=32,
                    max_members=4,
                )

    def test_rejects_zip_flags_before_materialization(self) -> None:
        for index, flag in enumerate((0x1, 0x8)):
            with self.subTest(flag=flag):
                archive = self.root / f"input-{index}.zip"
                with zipfile.ZipFile(archive, "w") as handle:
                    handle.writestr("pkg/file", b"bad")
                payload = bytearray(archive.read_bytes())
                local = payload.find(b"PK\x03\x04")
                central = payload.find(b"PK\x01\x02")
                struct.pack_into("<H", payload, local + 6, flag)
                struct.pack_into("<H", payload, central + 8, flag)
                archive.write_bytes(payload)

                with mock.patch.object(
                    zipfile,
                    "ZipFile",
                    side_effect=AssertionError("unsupported flags reached ZipFile"),
                ):
                    with self.assertRaises(ArchiveRejected):
                        extract_archive(
                            archive,
                            self.root / f"out-{index}",
                            max_decoded_bytes=32,
                            max_members=4,
                        )

    def test_crc_failure_is_rejected_and_removes_partial_output(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("pkg/first", b"good")
            handle.writestr("pkg/second", b"bad")
        payload = bytearray(archive.read_bytes())
        second = payload.find(b"bad")
        self.assertGreaterEqual(second, 0)
        payload[second : second + 3] = b"sad"
        archive.write_bytes(payload)
        destination = self.root / "out"

        with self.assertRaises(ArchiveRejected):
            extract_archive(
                archive,
                destination,
                max_decoded_bytes=32,
                max_members=4,
            )

        self.assertFalse(destination.exists())
        self.assertFalse((self.root / ".out.extracting").exists())

    def test_malformed_deflate_is_rejected_and_removes_partial_output(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w", compression=zipfile.ZIP_DEFLATED) as handle:
            handle.writestr("pkg/file", b"A" * 4096)
        payload = bytearray(archive.read_bytes())
        local = payload.find(b"PK\x03\x04")
        name_size, extra_size = struct.unpack_from("<HH", payload, local + 26)
        data_offset = local + 30 + name_size + extra_size
        payload[data_offset] = (payload[data_offset] & 0xF8) | 0x07
        archive.write_bytes(payload)
        destination = self.root / "out"

        with self.assertRaises(ArchiveRejected):
            extract_archive(
                archive,
                destination,
                max_decoded_bytes=8192,
                max_members=4,
            )

        self.assertFalse(destination.exists())
        self.assertFalse((self.root / ".out.extracting").exists())

    def test_zipfile_reads_from_the_preflight_handle(self) -> None:
        archive = self.root / "input.zip"
        with zipfile.ZipFile(archive, "w") as handle:
            handle.writestr("pkg/assay.exe", b"binary")
        real_zipfile = zipfile.ZipFile

        with mock.patch.object(
            zipfile,
            "ZipFile",
            side_effect=lambda source, *args, **kwargs: real_zipfile(
                source, *args, **kwargs
            ),
        ) as constructor:
            extract_archive(
                archive,
                self.root / "out",
                max_decoded_bytes=32,
                max_members=4,
            )

        source = constructor.call_args.args[0]
        self.assertTrue(hasattr(source, "read"), "ZipFile reopened the archive path")

    def test_does_not_materialize_member_table(self) -> None:
        archive = self.write_archive([("pkg/assay", b"binary", "file")])
        destination = self.root / "out"

        with mock.patch.object(
            tarfile.TarFile,
            "getmembers",
            side_effect=AssertionError("member table was materialized"),
        ):
            extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertEqual((destination / "pkg/assay").read_bytes(), b"binary")

    def test_rejects_traversal_before_materialization(self) -> None:
        archive = self.write_archive([("../escape", b"bad", "file")])
        destination = self.root / "out"

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, destination, max_decoded_bytes=32, max_members=4)

        self.assertFalse(destination.exists())
        self.assertFalse((self.root / "escape").exists())

    def test_rejects_link_member(self) -> None:
        archive = self.write_archive([("pkg/link", b"/tmp/target", "symlink")])

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=32, max_members=4)

    def test_rejects_duplicate_member(self) -> None:
        archive = self.write_archive(
            [("pkg/assay", b"one", "file"), ("pkg/assay", b"two", "file")]
        )

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=32, max_members=4)

    def test_rejects_member_count_ceiling(self) -> None:
        archive = self.write_archive(
            [(f"pkg/{index}", b"x", "file") for index in range(5)]
        )

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=32, max_members=4)

    def test_rejects_decoded_size_ceiling(self) -> None:
        archive = self.write_archive([("pkg/assay", b"12345", "file")])

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=4, max_members=4)

    def test_rejects_aggregate_decoded_size_ceiling(self) -> None:
        archive = self.write_archive(
            [("pkg/assay", b"123", "file"), ("pkg/README.md", b"456", "file")]
        )

        with self.assertRaises(ArchiveRejected):
            extract_archive(archive, self.root / "out", max_decoded_bytes=4, max_members=4)


if __name__ == "__main__":
    unittest.main()
