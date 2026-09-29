#!/usr/bin/env python3
from __future__ import annotations

import tempfile
import unittest
from pathlib import Path
from unittest import mock

from bounded_download import DownloadRejected, download


class FakeResponse:
    def __init__(self, payload: bytes, content_length: str | None = None) -> None:
        self.payload = payload
        self.offset = 0
        self.read_calls = 0
        self.headers = {}
        if content_length is not None:
            self.headers["Content-Length"] = content_length

    def __enter__(self) -> FakeResponse:
        return self

    def __exit__(self, *args: object) -> None:
        return None

    def read(self, size: int) -> bytes:
        self.read_calls += 1
        chunk = self.payload[self.offset : self.offset + size]
        self.offset += len(chunk)
        return chunk


class BoundedDownloadTests(unittest.TestCase):
    def setUp(self) -> None:
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)

    def tearDown(self) -> None:
        self.temp.cleanup()

    def test_representation_is_explicit_and_binary_default_is_preserved(self) -> None:
        for accept in (None, "application/vnd.github+json"):
            with self.subTest(accept=accept):
                path = self.root / ("binary" if accept is None else "metadata")
                expected = "application/octet-stream" if accept is None else accept
                def receive(request, timeout):
                    self.assertEqual(request.get_header("Accept"), expected)
                    self.assertEqual(timeout, 60)
                    return FakeResponse(b'{"id":7}')
                with mock.patch("bounded_download.urllib.request.urlopen", side_effect=receive):
                    options = {} if accept is None else {"accept": accept}
                    download("https://example.test/opaque", path, max_bytes=20, **options)
                self.assertEqual(path.read_bytes(), b'{"id":7}')

    def test_rejects_oversized_content_length_before_read(self) -> None:
        response = FakeResponse(b"payload", content_length="8")

        with mock.patch("bounded_download.urllib.request.urlopen", return_value=response):
            with self.assertRaises(DownloadRejected):
                download("https://example.test/asset", self.root / "asset", max_bytes=7)

        self.assertEqual(response.read_calls, 0)
        self.assertFalse((self.root / "asset").exists())

    def test_rejects_stream_that_exceeds_ceiling(self) -> None:
        response = FakeResponse(b"12345678")

        with mock.patch("bounded_download.urllib.request.urlopen", return_value=response):
            with self.assertRaises(DownloadRejected):
                download("https://example.test/asset", self.root / "asset", max_bytes=7)

        self.assertFalse((self.root / "asset").exists())
        self.assertFalse((self.root / ".asset.downloading").exists())

    def test_json_representation_keeps_stream_ceiling(self) -> None:
        with mock.patch("bounded_download.urllib.request.urlopen", return_value=FakeResponse(b"12345678")):
            with self.assertRaises(DownloadRejected):
                download("https://example.test/metadata", self.root / "metadata", max_bytes=7,
                         accept="application/vnd.github+json")
        self.assertFalse((self.root / "metadata").exists())
        self.assertFalse((self.root / ".metadata.downloading").exists())

    def test_atomically_publishes_bounded_download(self) -> None:
        response = FakeResponse(b"1234567", content_length="7")

        with mock.patch("bounded_download.urllib.request.urlopen", return_value=response):
            download("https://example.test/asset", self.root / "asset", max_bytes=7)

        self.assertEqual((self.root / "asset").read_bytes(), b"1234567")
        self.assertFalse((self.root / ".asset.downloading").exists())


if __name__ == "__main__":
    unittest.main()
