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


def verify_offline() -> int:
    octets = PINNED.read_bytes()
    digest = hashlib.sha256(octets).hexdigest()
    url_addr = MANIFEST["source_url"].rsplit("/", 1)[-1]
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
    req = urllib.request.Request(url, headers={"User-Agent": UA})
    with urllib.request.urlopen(req, timeout=30) as r:  # noqa: S310  https enforced above
        fresh = r.read()
    pinned = PINNED.read_bytes()
    same = fresh == pinned
    print(f"re-fetched     : {len(fresh)} bytes, sha256 {hashlib.sha256(fresh).hexdigest()}")
    print(f"identical to pinned: {same}")
    if not same:
        print("DIVERGENCE: the same content address served different bytes. Reported, not overwritten.")
    return 0 if same else 1


if __name__ == "__main__":
    sys.exit(refetch() if "--fetch" in sys.argv else verify_offline())
