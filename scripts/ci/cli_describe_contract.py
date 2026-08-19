"""Guard for the runtime CLI describe tree (#2178).

The old guard only checks that a listing is a describe document. The new guard
requires every shipping identity that belongs on that node to appear in the
listing, and forbids identities the bindings do not own. Values come from the
existing Rust constants, not from copies in this file.
"""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

CONST_RE = re.compile(r"const\s+([A-Z][A-Z0-9_]*)\s*:\s*&str\s*=\s*\"([^\"]+)\"")
BINDING_RE = re.compile(
    r'path:\s*"(?P<path>[^"]+)",\s*\n\s*identity:\s*(?P<const>[A-Z][A-Z0-9_]*)',
    re.MULTILINE,
)
BINDINGS_REL = Path("crates/assay-cli/src/cli/commands/describe/bindings.rs")
CLI_SRC_REL = Path("crates/assay-cli/src")


def shipping_constants(src_root: Path) -> dict[str, str]:
    found: dict[str, str] = {}
    for path in src_root.rglob("*.rs"):
        text = path.read_text(encoding="utf-8")
        for match in CONST_RE.finditer(text):
            found[match.group(1)] = match.group(2)
    return found


def binding_rows(repo: Path, constants: dict[str, str]) -> list[tuple[str, str]]:
    text = (repo / BINDINGS_REL).read_text(encoding="utf-8")
    rows: list[tuple[str, str]] = []
    for match in BINDING_RE.finditer(text):
        name = match.group("const")
        if name not in constants:
            raise SystemExit(f"binding {name} is not a shipping &str constant")
        rows.append((match.group("path"), constants[name]))
    if not rows:
        raise SystemExit("describe bindings table is empty")
    return rows


def belongs_on_node(binding_path: str, node_path: str) -> bool:
    if not node_path:
        return False
    if binding_path == node_path:
        return True
    rest = binding_path[len(node_path) :] if binding_path.startswith(node_path) else ""
    return rest.startswith("/") and "/" not in rest[1:]


def node_path(listing: dict) -> str:
    path = listing.get("path")
    if not isinstance(path, list) or not all(isinstance(part, str) for part in path):
        raise SystemExit("listing path must be an array of strings")
    return "/".join(path)


def old_guard(listing: dict) -> list[str]:
    problems: list[str] = []
    if not isinstance(listing.get("schema"), str) or not listing["schema"]:
        problems.append("describe listing is missing a schema field")
    if not isinstance(listing.get("commands"), list):
        problems.append("describe listing is missing a commands array")
    return problems


def new_guard(listing: dict, rows: list[tuple[str, str]]) -> list[str]:
    problems = old_guard(listing)
    identities = listing.get("identities")
    if not isinstance(identities, list) or not all(isinstance(item, str) for item in identities):
        problems.append("describe listing is missing an identities array")
        return problems
    node = node_path(listing)
    expected = {identity for path, identity in rows if belongs_on_node(path, node)}
    listed = set(identities)
    for identity in sorted(expected - listed):
        problems.append(f"parent listing omitted shipping identity {identity}")
    for identity in sorted(listed - expected):
        problems.append(f"parent listing leaked identity {identity}")
    return problems


def seed_identity_omission(listing: dict, identity: str) -> dict:
    mutated = json.loads(json.dumps(listing))
    identities = mutated.get("identities")
    if not isinstance(identities, list) or identity not in identities:
        raise SystemExit("cannot seed omission: listing does not carry the shipping identity")
    mutated["identities"] = [item for item in identities if item != identity]
    return mutated


def load_listing(path: Path) -> dict:
    return json.loads(path.read_text(encoding="utf-8"))


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--repo", type=Path, default=Path.cwd())
    parser.add_argument("--listing", type=Path, required=True)
    parser.add_argument("--guard", choices=("old", "new"), required=True)
    args = parser.parse_args(argv)
    constants = shipping_constants(args.repo / CLI_SRC_REL)
    rows = binding_rows(args.repo, constants)
    listing = load_listing(args.listing)
    problems = old_guard(listing) if args.guard == "old" else new_guard(listing, rows)
    for problem in problems:
        print(problem, file=sys.stderr)
    return 1 if problems else 0


if __name__ == "__main__":
    raise SystemExit(main())
