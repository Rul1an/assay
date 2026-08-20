#!/usr/bin/env python3
"""Load the one canonical conformance registry. Standard library only.

    python3 conformance/registry.py   # disk + INDEX vs registry; absence is not a pass

This is repository inventory coverage, not run_all execution completeness and
not semantic completeness of any corpus. It does not invoke run_all.py and
must not neutralize that runner's exit 3. Adequacy manifests are a different
domain.
"""

from __future__ import annotations

import json
import sys
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
REGISTRY_PATH = Path(__file__).resolve().parent / "registry.json"
SCHEMA = "assay.conformance.registry.v1"
MAX_REGISTRY_BYTES = 256 * 1024
KINDS = frozenset(("needs_candidate", "stdlib", "cargo", "external"))
POLICIES = frozenset(("required", "optional", "external-candidate"))
INDEX_FIELDS = ("index_corpus", "index_vectors", "index_runner", "index_maturity")
REQUIRED_FIELDS = ("id", "path", "kind", "policy", "maturity") + INDEX_FIELDS
SKIP_DIRS = frozenset(("adequacy", "tests"))
BEGIN_INVENTORY = "<!-- BEGIN REGISTRY INVENTORY -->"
END_INVENTORY = "<!-- END REGISTRY INVENTORY -->"


class RegistryError(Exception):
    """The registry file cannot be used. Absence is never a pass."""


def _reject_escape(path: str) -> None:
    if path.startswith(("http://", "https://")):
        return
    parts = Path(path).parts
    if Path(path).is_absolute() or ".." in parts:
        raise RegistryError("suite path escapes the repository: %r" % path)


def _validate_suite(suite: object, seen: set[str]) -> dict:
    if not isinstance(suite, dict):
        raise RegistryError("suite is %s, not an object" % type(suite).__name__)
    for field in REQUIRED_FIELDS:
        if field not in suite:
            raise RegistryError("suite missing %s" % field)
    ident = suite["id"]
    if not isinstance(ident, str) or not ident:
        raise RegistryError("suite id must be a non-empty string")
    if ident in seen:
        raise RegistryError("duplicate suite id: %s" % ident)
    seen.add(ident)
    if suite["kind"] not in KINDS:
        raise RegistryError("%s: unknown kind %r" % (ident, suite["kind"]))
    if suite["policy"] not in POLICIES:
        raise RegistryError("%s: unknown policy %r" % (ident, suite["policy"]))
    path = suite["path"]
    if not isinstance(path, str) or not path:
        raise RegistryError("%s: path must be a non-empty string" % ident)
    _reject_escape(path)
    vectors = suite.get("vectors")
    if vectors is not None and not isinstance(vectors, int):
        raise RegistryError("%s: vectors must be an int or null" % ident)
    kind = suite["kind"]
    if kind == "stdlib" and not isinstance(suite.get("expect_status"), str):
        raise RegistryError("%s: stdlib suite needs string expect_status" % ident)
    if kind == "cargo":
        for field in ("crate", "cargo_target_flag", "cargo_target"):
            if not isinstance(suite.get(field), str):
                raise RegistryError("%s: cargo suite needs string %s" % (ident, field))
    if kind in ("needs_candidate", "external") and not isinstance(suite.get("note"), str):
        raise RegistryError("%s: %s suite needs a string note" % (ident, kind))
    return suite


def load_registry(path: Path | None = None) -> dict:
    path = Path(path) if path is not None else REGISTRY_PATH
    if path.is_symlink():
        try:
            path.resolve().relative_to(REPO.resolve())
        except ValueError:
            raise RegistryError("registry symlink escapes the repository: %s" % path)
    if not path.is_file():
        raise RegistryError("registry missing: %s" % path)
    data = path.read_bytes()
    if len(data) > MAX_REGISTRY_BYTES:
        raise RegistryError("registry exceeds %d bytes" % MAX_REGISTRY_BYTES)
    try:
        doc = json.loads(data)
    except json.JSONDecodeError as exc:
        raise RegistryError("registry is not JSON: %s" % exc) from exc
    if not isinstance(doc, dict):
        raise RegistryError("registry is %s, not an object" % type(doc).__name__)
    if doc.get("schema") != SCHEMA:
        raise RegistryError("registry schema must be %s" % SCHEMA)
    suites = doc.get("suites")
    if not isinstance(suites, list):
        raise RegistryError("suites must be a list")
    seen: set[str] = set()
    doc["suites"] = [_validate_suite(s, seen) for s in suites]
    return doc


def load_suites(path: Path | None = None) -> list[dict]:
    return load_registry(path)["suites"]


def discover_published_roots(repo: Path) -> list[str]:
    """In-tree published roots, independent of the registry (the add/delete bite)."""
    roots: list[str] = []
    conformance = repo / "conformance"
    if conformance.is_dir():
        for child in sorted(conformance.iterdir()):
            if not child.is_dir() or child.name in SKIP_DIRS or child.name.startswith("."):
                continue
            if (child / "MANIFEST.json").is_file() or (child / "descriptor.json").is_file():
                roots.append(child.relative_to(repo).as_posix())
    examples = repo / "examples"
    if examples.is_dir():
        for child in sorted(examples.iterdir()):
            if child.is_dir() and child.name.endswith("-conformance"):
                roots.append(child.relative_to(repo).as_posix())
    return roots


def render_inventory_table(suites: list[dict]) -> str:
    lines = ["| Corpus | Vectors | Runner | Maturity |", "|---|---|---|---|"]
    for suite in suites:
        lines.append("| %s | %s | %s | %s |" % (
            suite["index_corpus"], suite["index_vectors"],
            suite["index_runner"], suite["index_maturity"]))
    return "\n".join(lines)


def index_inventory_section(text: str) -> str:
    start = text.find(BEGIN_INVENTORY)
    end = text.find(END_INVENTORY)
    if start < 0 or end < 0 or end <= start:
        raise RegistryError("INDEX inventory markers missing")
    return text[start + len(BEGIN_INVENTORY):end].strip()


def index_reasons(repo: Path, suites: list[dict]) -> list[str]:
    path = repo / "conformance/INDEX.md"
    if not path.is_file():
        return ["INDEX.md missing"]
    try:
        section = index_inventory_section(path.read_text(encoding="utf-8"))
    except RegistryError as exc:
        return [str(exc)]
    if section != render_inventory_table(suites):
        return ["INDEX inventory table is not the registry projection"]
    return []


def registry_completeness_reasons(
    repo: Path, registry_path: Path | None = None,
) -> list[str]:
    path = Path(registry_path) if registry_path is not None else repo / "conformance/registry.json"
    try:
        suites = load_suites(path)
    except RegistryError as exc:
        return [str(exc)]
    if not suites:
        return ["registry declares no suites"]
    reasons: list[str] = []
    registered: set[str] = set()
    for suite in suites:
        rel = suite["path"]
        if rel.startswith(("http://", "https://")):
            continue
        registered.add(rel.rstrip("/"))
        if not (repo / rel).exists():
            reasons.append("registered path missing: %s (%s)" % (suite["id"], rel))
    for root in discover_published_roots(repo):
        if root not in registered:
            reasons.append("unregistered published root: %s" % root)
    reasons.extend(index_reasons(repo, suites))
    return reasons


def main(argv: list[str] | None = None) -> int:
    if argv:
        sys.stderr.write("registry.py takes no flags; it only checks inventory coverage\n")
        return 2
    reasons = registry_completeness_reasons(REPO)
    if reasons:
        sys.stderr.write("registry incomplete (absence is not a pass):\n")
        for reason in reasons:
            sys.stderr.write("  %s\n" % reason)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
