#!/usr/bin/env python3
from __future__ import annotations

import io
import tarfile
import tempfile
import unittest
from pathlib import Path

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


if __name__ == "__main__":
    unittest.main()
