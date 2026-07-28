#!/usr/bin/env python3

import json
import sys
from typing import Any


def version_tuple(value: str) -> tuple[int, int, int]:
    parts = [int(part) for part in value.split(".")]
    return tuple((parts + [0, 0, 0])[:3])


def select_public_workspace_packages(
    metadata: dict[str, Any],
) -> list[dict[str, Any]]:
    workspace_members = set(metadata["workspace_members"])
    return sorted(
        (
            package
            for package in metadata["packages"]
            if package["id"] in workspace_members
            and package.get("source") is None
            and package.get("publish", ["default"]) != []
        ),
        key=lambda package: package["name"],
    )


def main() -> None:
    if len(sys.argv) != 2:
        raise SystemExit("usage: check_msrv_metadata.py EXPECTED_VERSION")

    metadata = json.load(sys.stdin)
    expected_text = sys.argv[1]
    expected = version_tuple(expected_text)
    public = select_public_workspace_packages(metadata)
    if not public:
        raise SystemExit("MSRV policy found no public workspace crates")

    bad_public = [
        f'{package["name"]}={package.get("rust_version") or "<missing>"}'
        for package in public
        if version_tuple(package.get("rust_version") or "0") != expected
    ]
    if bad_public:
        raise SystemExit(
            "public crate rust-version metadata must equal "
            f'{expected_text}: {", ".join(bad_public)}'
        )

    for package in public:
        print(package["name"])
    print(f"public-msrv-metadata={expected_text}", file=sys.stderr)
    print(f"public-crates={len(public)}", file=sys.stderr)


if __name__ == "__main__":
    main()
