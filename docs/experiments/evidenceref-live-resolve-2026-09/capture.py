#!/usr/bin/env python3
"""Verify the pinned capture, and optionally re-fetch it.

Offline by default: it checks that the pinned octets hash to the address in the URL they came from,
which is the only property that lets a third party's bytes sit in this repository at all. Nothing
here trusts the capturer.

  python3 capture.py            verify the pinned copy offline
  python3 capture.py --fetch    re-fetch the same address and compare byte for byte

A re-fetch that returns different bytes for the same address is a finding, not a refresh, so it is
reported and never written over the pinned copy.
"""
from __future__ import annotations

import hashlib
import json
import pathlib
import sys
import urllib.request

HERE = pathlib.Path(__file__).parent
UA = "assay-evidenceref-consumer/0.1 (+https://github.com/Rul1an/assay)"
MANIFEST = json.loads((HERE / "record" / "capture.json").read_text())
PINNED = HERE / MANIFEST["pinned_copy"]


EXPECTED_LEN = MANIFEST["byte_length"]


def address_in(url: str) -> str:
    """The content address the URL claims to serve: its last path segment."""
    return url.rsplit("/", 1)[-1]


def read_bounded(reader, limit: int = EXPECTED_LEN) -> bytes:
    """Read at most limit+1 bytes, so an oversize body is refused rather than buffered. A record
    whose length is pinned has no reason to be read unbounded, on the wire or from disk."""
    data = reader.read(limit + 1)
    if len(data) > limit:
        raise SystemExit(f"refusing a body longer than the pinned {limit} bytes")
    return data


def pinned_octets() -> bytes:
    with PINNED.open("rb") as fh:
        return read_bounded(fh)


def address_matches(octets: bytes, url: str) -> tuple[bool, str, str]:
    """Does the body's own digest equal the address the URL claims to serve? Kept pure so the
    wrong-address case is testable without the network. Comparing a fetch only against our own
    pinned copy would accept identical bytes served from any other address, which is precisely
    the binding this capture exists to establish."""
    got = hashlib.sha256(octets).hexdigest()
    claimed = address_in(url)
    return got == claimed, got, claimed


def verify_offline() -> int:
    octets = pinned_octets()
    digest = hashlib.sha256(octets).hexdigest()
    url_addr = address_in(MANIFEST["source_url"])
    ok = digest == url_addr == MANIFEST["content_address"].split(":", 1)[1]
    print(f"pinned bytes   : {len(octets)} bytes")
    print(f"sha256         : {digest}")
    print(f"address in URL : {url_addr}")
    print(f"self-verifying : {ok}")
    return 0 if ok and len(octets) == MANIFEST["byte_length"] else 1


def refetch() -> int:
    url = MANIFEST["source_url"]
    if not url.startswith("https://"):
        raise SystemExit("refusing a non-https source")
    # An explicit agent string is required: the endpoint answers 403 to the default Python-urllib
    # agent while serving the same address to curl. Worth knowing for a record published so that an
    # independent party can recompute it, since a bare stdlib fetch is the likeliest first attempt.
    claimed = address_in(url)
    if len(claimed) != 64 or claimed.strip("0123456789abcdef"):
        raise SystemExit(f"source_url does not end in a sha256 content address: {claimed!r}")
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:  # noqa: S310  https enforced above
        fresh = read_bounded(r)
    bound, got, claimed = address_matches(fresh, url)
    print(f"re-fetched     : {len(fresh)} bytes, sha256 {got}")
    print(f"address binding: {bound} (url claims {claimed})")
    if not bound:
        print("ADDRESS MISMATCH: the URL served bytes that are not the address it names.")
        return 1
    pinned = pinned_octets()
    same = fresh == pinned
    print(f"identical to pinned: {same}")
    if not same:
        print("DIVERGENCE: the same content address served different bytes. Reported, not overwritten.")
    return 0 if same else 1


if __name__ == "__main__":
    sys.exit(refetch() if "--fetch" in sys.argv else verify_offline())
