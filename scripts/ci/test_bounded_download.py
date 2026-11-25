#!/usr/bin/env python3
from __future__ import annotations

import email.message
import io
import tempfile
import unittest
import urllib.request
import urllib.response
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


class RedirectingHTTPS(urllib.request.BaseHandler):
    """Offline https handler: the first URL answers 302 to another host, the target serves bytes."""

    handler_order = 100  # ahead of the real HTTPSHandler, so nothing reaches the network

    def __init__(self, first: str, target: str, payload: bytes) -> None:
        self.first, self.target, self.payload = first, target, payload
        self.seen: list[tuple[str, dict[str, str]]] = []

    def https_open(self, request: urllib.request.Request) -> urllib.response.addinfourl:
        self.seen.append((request.full_url, dict(request.header_items())))
        headers = email.message.Message()
        if request.full_url == self.first:
            headers["Location"] = self.target
            response = urllib.response.addinfourl(io.BytesIO(b""), headers, request.full_url, 302)
            response.msg = "Found"
            return response
        if request.full_url != self.target:
            raise AssertionError(f"unexpected request to {request.full_url}")
        headers["Content-Length"] = str(len(self.payload))
        response = urllib.response.addinfourl(io.BytesIO(self.payload), headers, request.full_url, 200)
        response.msg = "OK"
        return response


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

    def test_token_is_sent_only_when_given(self) -> None:
        # The post-publication metadata re-read authenticates; asset downloads stay anonymous.
        for token in (None, "", "t0ken"):
            with self.subTest(token=token):
                path = self.root / f"auth-{token!r}"
                def receive(request, timeout):
                    header = request.get_header("Authorization")
                    if token:
                        self.assertEqual(header, f"Bearer {token}")
                    else:
                        self.assertIsNone(header)
                    return FakeResponse(b"{}")
                with mock.patch("bounded_download.urllib.request.urlopen", side_effect=receive):
                    download("https://example.test/opaque", path, max_bytes=20, token=token)
                self.assertEqual(path.read_bytes(), b"{}")

    def test_redirect_never_carries_the_token(self) -> None:
        # urllib copies ordinary request headers onto a redirect's new request, so a token kept
        # there would follow a redirect to whatever host it names. The token must reach the first
        # request only. Dummy value, real urllib redirect handling, no network.
        first = "https://api.github.com/repos/Rul1an/assay/releases/tags/v0.0.0"
        target = "https://objects.example.test/blob"
        for token in ("dummy-token", None):
            with self.subTest(token=token):
                handler = RedirectingHTTPS(first, target, b"{}")
                opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), handler)
                path = self.root / f"redirect-{token!r}"
                with mock.patch(
                    "bounded_download.urllib.request.urlopen",
                    side_effect=lambda request, timeout: opener.open(request, timeout=timeout),
                ):
                    download(first, path, max_bytes=20, accept="application/vnd.github+json", token=token)
                self.assertEqual(path.read_bytes(), b"{}")
                self.assertEqual([url for url, _ in handler.seen], [first, target])
                initial, redirected = (headers for _, headers in handler.seen)
                self.assertEqual(initial.get("Authorization"), f"Bearer {token}" if token else None)
                self.assertNotIn("Authorization", redirected)
                # Control on the same path: ordinary headers do follow, so a copied token would show.
                self.assertEqual(redirected.get("User-agent"), "assay-release-verifier")

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
